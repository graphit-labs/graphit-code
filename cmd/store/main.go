// Command graphit-store runs the Task and Memory maintenance the daemon schedules:
// `task maintain`, `memory maintain`, `memory embed` and `memory index`, each optionally
// with --user for memory (and --reset for index).
//
// The daemon runs these for every active project and for the user scope, so they start
// far more often than any interactive command. This binary links the table store and
// the embedding client only — no parsers, graph database or Hub registry. The launcher
// routes those exact invocations here when the daemon (or another managed parent)
// starts them; an interactive run keeps going to the core. Anything this binary cannot
// serve — a Broker provider whose scoped storage only the Hub registry can authorize,
// or a form it does not know — is handed to the core unchanged.
package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"syscall"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/storework"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

// errUseCore asks for the invocation to be handed to the core.
var errUseCore = errors.New("handled by the core")

// coreBinary locates the core; tests replace it.
var coreBinary = clirun.SiblingCore

// usesScopedStorage reports whether the active provider's storage needs the Hub
// registry to authorize a project; tests replace it.
var usesScopedStorage = func(ctx context.Context) bool {
	snapshot, err := auth.ResolveActive(ctx)
	return err == nil && auth.UsesScopedS3(snapshot.Provider)
}

func run(args []string, stdout, stderr io.Writer) int {
	work, ok := parse(args)
	if !ok {
		return runCore(args, stdout, stderr)
	}
	if clirun.IsBackground() {
		runtime.GOMAXPROCS(sysutil.CPUBudget())
		_ = sysutil.LowerPriority()
	}
	slogutil.InitFileLogger(brand.GlobalDir())
	defer slogutil.CloseFileLogger()

	signals := make(chan os.Signal, 1)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-signals
		output.Interrupted()
	}()

	ctx := context.Background()
	if usesScopedStorage(ctx) {
		return runCore(args, stdout, stderr)
	}
	wd, err := os.Getwd()
	if err == nil {
		wd, err = filepath.Abs(wd)
	}
	if err == nil {
		err = work(ctx, wd)
	}
	switch {
	case errors.Is(err, errUseCore), errors.Is(err, hubaccess.ErrScopedAuthorizerUnavailable):
		// Authorization happens before any write, so the core starts from the same state.
		return runCore(args, stdout, stderr)
	case err != nil:
		// Exactly how the core reports a failed command.
		slogutil.CloseFileLogger()
		output.Fatal("%s", err)
	}
	return 0
}

// parse maps an invocation to its work, or reports that this binary does not serve it.
func parse(args []string) (func(ctx context.Context, projectDir string) error, bool) {
	if len(args) == 2 && args[0] == "task" && args[1] == "maintain" {
		return func(ctx context.Context, projectDir string) error {
			if _, err := os.Stat(filepath.Join(projectDir, brand.LockFileName())); os.IsNotExist(err) {
				return fmt.Errorf("this project is not managed by %s — run '%s init' first", brand.DisplayName, brand.BinName())
			}
			return storework.MaintainTasks(ctx, projectDir)
		}, true
	}
	if len(args) < 2 || args[0] != "memory" {
		return nil, false
	}
	user, reset := false, false
	for _, flag := range args[2:] {
		switch {
		case flag == "--user" && !user:
			user = true
		case flag == "--reset" && !reset && args[1] == "index":
			reset = true
		default:
			return nil, false
		}
	}
	switch args[1] {
	case "maintain":
		return func(ctx context.Context, dir string) error { return storework.MaintainMemory(ctx, dir, user) }, true
	case "embed":
		return func(ctx context.Context, dir string) error { return storework.EmbedMemory(ctx, dir, user) }, true
	case "index":
		return func(ctx context.Context, dir string) error { return storework.IndexMemory(ctx, dir, user, reset) }, true
	}
	return nil, false
}

// runCore runs the same invocation in the core binary next to this one.
func runCore(args []string, stdout, stderr io.Writer) int {
	core, err := coreBinary()
	if err != nil {
		_, _ = fmt.Fprintln(stderr, err)
		return 1
	}
	cmd := exec.Command(core, args...)
	cmd.Stdin = os.Stdin
	cmd.Stdout, cmd.Stderr = stdout, stderr
	if err := cmd.Run(); err != nil {
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			return exitErr.ExitCode()
		}
		_, _ = fmt.Fprintln(stderr, err)
		return 1
	}
	return 0
}
