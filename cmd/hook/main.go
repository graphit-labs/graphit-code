// Command graphit-hook runs the Agent lifecycle hooks (`graphit _session-hook`).
//
// Agents fire hooks at every session boundary and tool call, so this process starts far
// more often than any other. It links no store, parser or model: it reads the event,
// resolves the project and renders the payload itself from the lockfile and installed
// files. Only the store-backed half of an event — the Task lifecycle effect and
// mandatory memory — goes to the running daemon, and an event that needs neither never
// leaves this process. Without a reachable daemon it hands the very same invocation to
// the core, which handles it entirely. The launcher runs this binary for
// `_session-hook`; the agent configuration keeps calling `graphit _session-hook`.
package main

import (
	"bytes"
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("_session-hook", flag.ContinueOnError)
	flags.SetOutput(stderr)
	format := flags.String("format", "", "session hook output format")
	projectDirFlag := flags.String("project-dir", "", "starting directory for Graphit project discovery")
	syncAfter := flags.Bool("sync", false, "dispatch a full project sync asynchronously")
	if err := flags.Parse(args); err != nil {
		return 2
	}
	if *format == "" {
		fmt.Fprintln(stderr, `required flag(s) "format" not set`)
		return 2
	}

	input, err := sessionhook.ReadInput(stdin, *format)
	if err != nil {
		fmt.Fprintf(stderr, "reading session hook input: %v\n", err)
		return 1
	}
	projectDir := sessionhook.ResolveProjectDir(*projectDirFlag, input)
	if *syncAfter && projectDir == "" {
		fmt.Fprintln(stderr, "no Graphit project lockfile was found from the hook input")
		return 1
	}

	payload, err := sessioncontext.Render(context.Background(), projectDir, *format, input, applyThroughDaemon)
	if errors.Is(err, sessionhook.ErrDaemonUnavailable) {
		return runCore(args, input, stdout, stderr)
	}
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	if *syncAfter {
		// Through the launcher, so the sync runs the core of this same installation.
		if err := clirun.Start(clirun.Spec{Dir: projectDir, Args: []string{"sync"}}); err != nil {
			fmt.Fprintf(stderr, "starting asynchronous project sync: %v\n", err)
			return 1
		}
	}
	if len(payload) > 0 {
		_, _ = fmt.Fprintln(stdout, string(payload))
	}
	return 0
}

// applyThroughDaemon asks the running daemon for the store-backed half of the event. The
// hook never starts the daemon itself — the core command did not either — so a stopped
// daemon means the core handles the event instead.
func applyThroughDaemon(ctx context.Context, req sessionhook.StoreRequest) (sessionhook.StoreResult, error) {
	port, err := readTrimmed(daemonctl.PortFilePath())
	portNum, convErr := strconv.Atoi(port)
	if err != nil || convErr != nil || portNum <= 0 {
		return sessionhook.StoreResult{}, fmt.Errorf("%w: no daemon port", sessionhook.ErrDaemonUnavailable)
	}
	key, err := readTrimmed(daemonctl.KeyFilePath())
	if err != nil || key == "" {
		return sessionhook.StoreResult{}, fmt.Errorf("%w: no daemon key", sessionhook.ErrDaemonUnavailable)
	}
	return sessionhook.ApplyRemote(ctx, portNum, key, req)
}

func readTrimmed(path string) (string, error) {
	data, err := os.ReadFile(path)
	return strings.TrimSpace(string(data)), err
}

// runCore hands the invocation to the core binary next to this one, replaying the
// event already read from stdin. It never goes back through the launcher, which would
// route the call to this binary again.
func runCore(args []string, input []byte, stdout, stderr io.Writer) int {
	core, err := coreBinary()
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	cmd := exec.Command(core, append([]string{"_session-hook"}, args...)...)
	cmd.Stdin = bytes.NewReader(input)
	cmd.Stdout, cmd.Stderr = stdout, stderr
	if err := cmd.Run(); err != nil {
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			return exitErr.ExitCode()
		}
		fmt.Fprintln(stderr, err)
		return 1
	}
	return 0
}

// coreBinary locates the core; tests replace it.
var coreBinary = clirun.SiblingCore
