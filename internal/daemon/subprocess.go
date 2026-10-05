package daemon

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

// Core jobs run as scoped services in this daemon. Dedicated runtime binaries keep
// their process boundary; both paths share scheduling, cancellation and log sinks.

// cliRunner starts the children. Tests replace it to record invocations.
var cliRunner clirun.Runner = clirun.Default

var coreRunner clirun.Runner = inProcessRunner{}

// projectLogPath is the per-project log every child of that project writes to.
func projectLogPath(projectDir string) string {
	return brand.ProjectRuntimePath(projectDir, "daemon", "daemon.log")
}

// globalLogPath is the log of a machine-wide child such as a model server.
func globalLogPath(name string) string {
	return filepath.Join(GlobalDaemonDir(), name+".log")
}

func openAppendLog(path string) (*os.File, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	return os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
}

// runChild runs one CLI invocation for dir, appending its output to logPath. A child
// that cannot open its log still runs; the output is discarded rather than the work.
func runChild(ctx context.Context, dir, logPath string, stdin io.Reader, args ...string) error {
	spec := clirun.Spec{Dir: dir, Args: args, Stdin: stdin, Background: true, Env: versionEnv(dir)}
	if f, err := openAppendLog(logPath); err == nil {
		defer func() { _ = f.Close() }()
		_, _ = fmt.Fprintf(f, "%s [daemon] running: %s %v\n", time.Now().UTC().Format(time.RFC3339), brand.BinName(), args)
		spec.Stdout, spec.Stderr = f, f
	}
	return jobRunner(ctx, args).Run(ctx, spec)
}

// versionEnv pins a project child to the version checked out in its directory when it
// starts, so one run never switches stores halfway through a branch change, and a
// version pinned in the daemon's own environment never reaches a project's child.
func versionEnv(dir string) []string {
	info, _ := projectversion.Resolve(dir, "")
	return []string{projectversion.EnvVersion() + "=" + info.Version}
}

// runProjectChild runs a CLI invocation scoped to projectDir.
func runProjectChild(ctx context.Context, projectDir string, stdin io.Reader, args ...string) error {
	return runChild(ctx, projectDir, projectLogPath(projectDir), stdin, args...)
}

// outputProjectChild runs a CLI invocation scoped to projectDir and returns its stdout;
// stderr still goes to the project log.
func outputProjectChild(ctx context.Context, projectDir string, args ...string) ([]byte, error) {
	var out bytes.Buffer
	spec := clirun.Spec{Dir: projectDir, Args: args, Background: true, Stdout: &out, Env: versionEnv(projectDir)}
	if f, err := openAppendLog(projectLogPath(projectDir)); err == nil {
		defer func() { _ = f.Close() }()
		spec.Stderr = f
	}
	err := jobRunner(ctx, args).Run(ctx, spec)
	return out.Bytes(), err
}
