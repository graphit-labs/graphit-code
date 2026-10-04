// Package clirun starts the graphit CLI as a child process.
//
// Every piece of project work the daemon schedules — and every step `sync` delegates —
// runs through here, so each one is its own operating-system process: visible in ps or
// Task Manager with its own CPU and memory, killable on its own, and unable to take the
// daemon down with it. The child always runs with its working directory set to the
// project it works on; the CLI resolves the project from the working directory, so that
// is what keeps a child scoped to one project.
//
// A managed tree is cancelled as a unit. On Unix the outermost managed child leads a new
// process group and cancellation signals the whole group (SIGTERM, then SIGKILL after a
// grace period); descendants started by that child stay in its group. On Windows the
// tree is ended with taskkill /T.
package clirun

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// terminateGrace is how long a cancelled child has between the polite stop and the kill.
const terminateGrace = 10 * time.Second

// stderrTailBytes bounds the stderr excerpt carried in an ExitError.
const stderrTailBytes = 2048

// SubprocessEnv marks a process started through this package. Its presence tells the
// child it is managed: it must not start the daemon or tray itself, and descendants it
// starts join its process group instead of leading their own.
func SubprocessEnv() string { return brand.EnvVar("SUBPROCESS") }

// BackgroundEnv marks a managed tree started by the daemon. A background child lowers
// its own scheduling priority and stays inside the CPU budget, whoever its parent is.
func BackgroundEnv() string { return brand.EnvVar("BACKGROUND") }

// IsSubprocess reports whether this process was started through this package.
func IsSubprocess() bool { return os.Getenv(SubprocessEnv()) == "1" }

// IsBackground reports whether this process belongs to a daemon-started tree.
func IsBackground() bool { return os.Getenv(BackgroundEnv()) == "1" }

// Spec describes one CLI invocation.
type Spec struct {
	// Dir is the child's working directory: the project root for project work. Required.
	Dir string
	// Args are the CLI arguments, without the executable.
	Args []string
	// Stdin feeds the child's standard input; nil gives it none.
	Stdin io.Reader
	// Stdout and Stderr receive the child's output; nil discards it. An *os.File is
	// handed to the child directly, without a copying goroutine.
	Stdout io.Writer
	Stderr io.Writer
	// Inherit gives the child this process's standard streams and keeps it in this
	// process's console and process group, for interactive commands such as `sync`.
	// It overrides Stdin, Stdout and Stderr.
	Inherit bool
	// Background marks the tree as daemon work (see BackgroundEnv).
	Background bool
	// ExitWithParent hands the child a pipe on stdin that this process never writes and
	// never closes; when this process dies the pipe reaches EOF and a child that calls
	// WatchParent stops itself. For long-lived children such as model servers.
	ExitWithParent bool
	// Env adds KEY=VALUE pairs to the inherited environment.
	Env []string
	// Executable overrides the CLI binary; empty resolves it with Executable.
	Executable string
}

// ExitError reports a child that ran and failed.
type ExitError struct {
	Args []string
	Code int
	// Tail is the end of the child's stderr when it was captured.
	Tail string
	Err  error
}

func (e *ExitError) Error() string {
	msg := fmt.Sprintf("%s %s: exit %d", brand.BinName(), strings.Join(e.Args, " "), e.Code)
	if tail := strings.TrimSpace(e.Tail); tail != "" {
		msg += ": " + tail
	}
	return msg
}

func (e *ExitError) Unwrap() error { return e.Err }

// Runner runs one invocation to completion. Code that schedules CLI work depends on this
// interface so tests can record invocations instead of starting processes.
type Runner interface {
	Run(ctx context.Context, spec Spec) error
}

// Default is the Runner that starts real processes.
var Default Runner = processRunner{}

type processRunner struct{}

func (processRunner) Run(ctx context.Context, spec Spec) error { return Run(ctx, spec) }

// RunnerFunc adapts a function to Runner.
type RunnerFunc func(ctx context.Context, spec Spec) error

func (f RunnerFunc) Run(ctx context.Context, spec Spec) error { return f(ctx, spec) }

// Executable resolves the CLI binary children run: the stable launcher when the launcher
// provided its path, otherwise this binary, otherwise the first one on PATH.
func Executable() (string, error) {
	if launcher := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); launcher != "" {
		if info, err := os.Stat(launcher); err == nil && !info.IsDir() {
			return filepath.Abs(launcher)
		}
	}
	if exe, err := os.Executable(); err == nil {
		if resolved, err := filepath.EvalSymlinks(exe); err == nil {
			return resolved, nil
		}
		return exe, nil
	}
	if exe, err := exec.LookPath(brand.BinName()); err == nil {
		return filepath.Abs(exe)
	}
	return "", fmt.Errorf("cannot locate the %s executable", brand.BinName())
}

// Run starts the invocation and waits for it. Cancelling ctx stops the child and its
// descendants. A child that exits non-zero yields an *ExitError.
func Run(ctx context.Context, spec Spec) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	cmd, err := build(spec)
	if err != nil {
		return err
	}
	tail := &tailBuffer{max: stderrTailBytes}
	if !spec.Inherit {
		cmd.Stderr = teeStderr(spec.Stderr, tail)
	}
	var parentPipe *os.File
	if spec.ExitWithParent && !spec.Inherit {
		r, w, err := os.Pipe()
		if err != nil {
			return fmt.Errorf("parent pipe: %w", err)
		}
		cmd.Stdin = r
		parentPipe = w
		defer func() { _ = r.Close() }()
	}

	tree := configureTree(cmd, !spec.Inherit && !IsSubprocess(), spec.Inherit)
	if err := cmd.Start(); err != nil {
		if parentPipe != nil {
			_ = parentPipe.Close()
		}
		return fmt.Errorf("start %s %s: %w", brand.BinName(), strings.Join(spec.Args, " "), err)
	}
	// The write end stays open in this process for the child's lifetime; it is closed
	// here only after the child has gone.
	if parentPipe != nil {
		defer func() { _ = parentPipe.Close() }()
	}

	waitErr := make(chan error, 1)
	go func() { waitErr <- cmd.Wait() }()

	var err2 error
	select {
	case err2 = <-waitErr:
	case <-ctx.Done():
		tree.stop()
		err2 = <-waitErr
	}
	tree.done()

	if ctxErr := ctx.Err(); ctxErr != nil {
		return ctxErr
	}
	if err2 != nil {
		var exitErr *exec.ExitError
		if errors.As(err2, &exitErr) {
			return &ExitError{Args: spec.Args, Code: exitErr.ExitCode(), Tail: tail.String(), Err: err2}
		}
		return fmt.Errorf("%s %s: %w", brand.BinName(), strings.Join(spec.Args, " "), err2)
	}
	return nil
}

// Output runs the invocation and returns its standard output.
func Output(ctx context.Context, spec Spec) ([]byte, error) {
	var out bytes.Buffer
	spec.Inherit = false
	spec.Stdout = &out
	err := Run(ctx, spec)
	return out.Bytes(), err
}

// Start launches the invocation detached from this process — its own session, no
// console window, no inherited streams — and does not wait. For work that must outlive
// an interactive command, such as `sync` handing its heavy phase to the background.
func Start(spec Spec) error {
	spec.Inherit = false
	cmd, err := build(spec)
	if err != nil {
		return err
	}
	cmd.Stdin = nil
	sysutil.DetachProcess(cmd)
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start %s %s: %w", brand.BinName(), strings.Join(spec.Args, " "), err)
	}
	go func() { _ = cmd.Wait() }()
	return nil
}

// WatchParent stops this process's work when the parent that started it with
// ExitWithParent goes away. It returns immediately when there is no such pipe.
func WatchParent(cancel context.CancelFunc) {
	if !IsSubprocess() || os.Getenv(brand.EnvVar("PARENT_PIPE")) != "1" {
		return
	}
	go func() {
		_, _ = io.Copy(io.Discard, os.Stdin)
		cancel()
	}()
}

func build(spec Spec) (*exec.Cmd, error) {
	if spec.Dir == "" {
		return nil, errors.New("clirun: Spec.Dir is required")
	}
	exe := spec.Executable
	if exe == "" {
		var err error
		if exe, err = Executable(); err != nil {
			return nil, err
		}
	}
	cmd := exec.Command(exe, spec.Args...)
	cmd.Dir = spec.Dir
	env := os.Environ()
	env = append(env, SubprocessEnv()+"=1")
	if spec.Background {
		env = append(env, BackgroundEnv()+"=1")
	}
	if spec.ExitWithParent && !spec.Inherit {
		env = append(env, brand.EnvVar("PARENT_PIPE")+"=1")
	}
	env = append(env, spec.Env...)
	cmd.Env = env
	if spec.Inherit {
		cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
		return cmd, nil
	}
	cmd.Stdin = spec.Stdin
	cmd.Stdout = orDiscard(spec.Stdout)
	cmd.Stderr = orDiscard(spec.Stderr)
	return cmd, nil
}

func orDiscard(w io.Writer) io.Writer {
	if w == nil {
		return io.Discard
	}
	return w
}

// teeStderr keeps the stderr tail for error reports while still delivering the stream
// to its destination.
func teeStderr(dst io.Writer, tail *tailBuffer) io.Writer {
	if dst == nil {
		return tail
	}
	return io.MultiWriter(dst, tail)
}

// tailBuffer retains the last max bytes written to it.
type tailBuffer struct {
	mu  sync.Mutex
	buf []byte
	max int
}

func (t *tailBuffer) Write(p []byte) (int, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.buf = append(t.buf, p...)
	if over := len(t.buf) - t.max; over > 0 {
		t.buf = append(t.buf[:0], t.buf[over:]...)
	}
	return len(p), nil
}

func (t *tailBuffer) String() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return string(t.buf)
}

// SiblingCore locates the core binary installed next to this executable. A lightweight
// runtime binary hands an invocation it cannot serve to the core directly, never back
// through the launcher, which would route it to the lightweight binary again.
func SiblingCore() (string, error) {
	self, err := os.Executable()
	if err != nil {
		return "", fmt.Errorf("locating the %s core: %w", brand.BinName(), err)
	}
	if resolved, err := filepath.EvalSymlinks(self); err == nil {
		self = resolved
	}
	name := brand.BinName() + "-core"
	if runtime.GOOS == "windows" {
		name += ".exe"
	}
	core := filepath.Join(filepath.Dir(self), name)
	if _, err := os.Stat(core); err != nil {
		return "", fmt.Errorf("locating the %s core next to %s: %w", brand.BinName(), self, err)
	}
	return core, nil
}
