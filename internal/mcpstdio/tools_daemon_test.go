package mcpstdio

import (
	"errors"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func stubDaemonLifecycle(t *testing.T, ensure func() (bool, error), swap func() error, pid func() (int, bool)) {
	t.Helper()
	previousEnsure, previousSwap, previousPID := ensureDaemonRunning, triggerDaemonSwap, livingDaemonPID
	t.Cleanup(func() {
		ensureDaemonRunning, triggerDaemonSwap, livingDaemonPID = previousEnsure, previousSwap, previousPID
	})
	ensureDaemonRunning, triggerDaemonSwap, livingDaemonPID = ensure, swap, pid
}

func resultText(t *testing.T, result *mcp.CallToolResult) string {
	t.Helper()
	if result == nil {
		t.Fatal("tool returned no result")
	}
	var text strings.Builder
	for _, content := range result.Content {
		if part, ok := content.(*mcp.TextContent); ok {
			text.WriteString(part.Text)
		}
	}
	return text.String()
}

func TestStartingAnAlreadyRunningDaemonLeavesItAlone(t *testing.T) {
	swapped := false
	stubDaemonLifecycle(t,
		func() (bool, error) { return false, nil },
		func() error { swapped = true; return nil },
		func() (int, bool) { return 4321, true },
	)

	result, _, err := startDaemon()
	if err != nil {
		t.Fatalf("starting an already running daemon: %v", err)
	}
	text := resultText(t, result)
	if !strings.Contains(text, "already running") || !strings.Contains(text, "4321") {
		t.Errorf("reply = %q, want it to report PID 4321 as already running", text)
	}
	if swapped {
		t.Error("starting the daemon triggered a restart, want it to leave the process alone")
	}
}

func TestStartingAStoppedDaemonReportsTheNewProcess(t *testing.T) {
	stubDaemonLifecycle(t,
		func() (bool, error) { return true, nil },
		func() error { t.Fatal("start must not trigger a restart"); return nil },
		func() (int, bool) { return 555, true },
	)

	result, _, err := startDaemon()
	if err != nil {
		t.Fatalf("starting a stopped daemon: %v", err)
	}
	if text := resultText(t, result); !strings.Contains(text, "started") || !strings.Contains(text, "555") {
		t.Errorf("reply = %q, want it to report PID 555 as started", text)
	}
}

func TestAFailedStartIsReportedInsteadOfSwallowed(t *testing.T) {
	stubDaemonLifecycle(t,
		func() (bool, error) { return false, errors.New("no executable") },
		func() error { return nil },
		func() (int, bool) { return 0, false },
	)

	if _, _, err := startDaemon(); err == nil {
		t.Fatal("a failing start returned no error")
	}
}

func TestRestartAnswersWhileTheDaemonIsStillUp(t *testing.T) {
	swaps := 0
	stubDaemonLifecycle(t,
		func() (bool, error) {
			t.Fatal("restart must not go through the start path while a daemon is live")
			return false, nil
		},
		func() error { swaps++; return nil },
		func() (int, bool) { return 9999, true },
	)

	result, _, err := restartDaemon()
	if err != nil {
		t.Fatalf("restarting the daemon: %v", err)
	}
	// The point of the fix: the reply exists while the outgoing process is still alive, so it is
	// never waiting on the teardown that would kill the connection carrying it.
	if pid, running := livingDaemonPID(); !running || pid != 9999 {
		t.Fatal("restart waited for the daemon to die before replying")
	}
	if swaps != 1 {
		t.Errorf("restart handed the swap over %d time(s), want exactly 1", swaps)
	}
	if text := resultText(t, result); !strings.Contains(text, "9999") || !strings.Contains(text, "separate process") {
		t.Errorf("reply = %q, want it to name PID 9999 and say the swap runs elsewhere", text)
	}
}

func TestRestartingWithNoDaemonJustStartsOne(t *testing.T) {
	started, swapped := false, false
	stubDaemonLifecycle(t,
		func() (bool, error) { started = true; return true, nil },
		func() error { swapped = true; return nil },
		func() (int, bool) { return 0, false },
	)

	if _, _, err := restartDaemon(); err != nil {
		t.Fatalf("restarting with nothing running: %v", err)
	}
	if !started {
		t.Error("restart did not start a daemon when none was running")
	}
	if swapped {
		t.Error("restart signalled a process swap when there was nothing to replace")
	}
}

func TestAFailedSwapIsReportedInsteadOfSwallowed(t *testing.T) {
	stubDaemonLifecycle(t,
		func() (bool, error) { return false, nil },
		func() error { return errors.New("cannot spawn") },
		func() (int, bool) { return 4242, true },
	)

	if _, _, err := restartDaemon(); err == nil {
		t.Fatal("a failing swap returned no error")
	}
}
