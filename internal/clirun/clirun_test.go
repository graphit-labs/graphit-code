package clirun

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const helperEnv = "CLIRUN_TEST_HELPER"

// TestMain doubles as the child process: with helperEnv set, the test binary plays the
// CLI instead of running tests, so these tests start real processes on every platform.
func TestMain(m *testing.M) {
	if mode := os.Getenv(helperEnv); mode != "" {
		os.Exit(runHelper(mode))
	}
	os.Exit(m.Run())
}

func runHelper(mode string) int {
	switch mode {
	case "pwd":
		wd, _ := os.Getwd()
		fmt.Print(wd)
		return 0
	case "env":
		fmt.Printf("%s|%s", os.Getenv(SubprocessEnv()), os.Getenv(BackgroundEnv()))
		return 0
	case "fail":
		fmt.Fprint(os.Stderr, "boom from child")
		return 3
	case "stdin":
		data := make([]byte, 64)
		n, _ := os.Stdin.Read(data)
		fmt.Print(strings.ToUpper(string(data[:n])))
		return 0
	case "sleep":
		time.Sleep(time.Minute)
		return 0
	case "tree":
		// Start a grandchild the way a nested CLI command would, record its pid, then
		// wait for cancellation.
		child := exec.Command(os.Args[0])
		child.Env = append(os.Environ(), helperEnv+"=sleep")
		if err := child.Start(); err != nil {
			return 9
		}
		_ = os.WriteFile(os.Getenv("CLIRUN_TEST_PIDFILE"), []byte(fmt.Sprint(child.Process.Pid)), 0o644)
		time.Sleep(time.Minute)
		return 0
	case "parent-watch":
		ctx, cancel := context.WithCancel(context.Background())
		WatchParent(cancel)
		select {
		case <-ctx.Done():
			_ = os.WriteFile(os.Getenv("CLIRUN_TEST_PIDFILE"), []byte("parent gone"), 0o644)
			return 0
		case <-time.After(time.Minute):
			return 1
		}
	}
	return 2
}

func helperSpec(t *testing.T, mode string) Spec {
	t.Helper()
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	return Spec{Dir: t.TempDir(), Executable: exe, Env: []string{helperEnv + "=" + mode}}
}

func TestRunUsesTheProjectDirectoryAsWorkingDirectory(t *testing.T) {
	spec := helperSpec(t, "pwd")
	out, err := Output(context.Background(), spec)
	if err != nil {
		t.Fatalf("Output: %v", err)
	}
	got, _ := filepath.EvalSymlinks(string(out))
	want, _ := filepath.EvalSymlinks(spec.Dir)
	if got != want {
		t.Fatalf("child working directory = %q, want %q", got, want)
	}
}

func TestRunMarksTheChildAsManaged(t *testing.T) {
	spec := helperSpec(t, "env")
	spec.Background = true
	out, err := Output(context.Background(), spec)
	if err != nil {
		t.Fatalf("Output: %v", err)
	}
	if string(out) != "1|1" {
		t.Fatalf("child markers = %q, want subprocess and background set", out)
	}
}

func TestRunReportsTheExitCodeAndStderrTail(t *testing.T) {
	err := Run(context.Background(), helperSpec(t, "fail"))
	var exitErr *ExitError
	if !errors.As(err, &exitErr) {
		t.Fatalf("Run error = %v, want *ExitError", err)
	}
	if exitErr.Code != 3 {
		t.Errorf("exit code = %d, want 3", exitErr.Code)
	}
	if !strings.Contains(exitErr.Error(), "boom from child") {
		t.Errorf("error %q lacks the child's stderr", exitErr.Error())
	}
}

func TestRunFeedsStdin(t *testing.T) {
	spec := helperSpec(t, "stdin")
	spec.Stdin = strings.NewReader("changes")
	out, err := Output(context.Background(), spec)
	if err != nil {
		t.Fatalf("Output: %v", err)
	}
	if string(out) != "CHANGES" {
		t.Fatalf("stdout = %q, want CHANGES", out)
	}
}

func TestRunRequiresADirectory(t *testing.T) {
	if err := Run(context.Background(), Spec{Args: []string{"sync"}}); err == nil {
		t.Fatal("Run without Dir succeeded; a child must always be scoped to a directory")
	}
}

func TestCancelStopsTheChildAndItsDescendants(t *testing.T) {
	spec := helperSpec(t, "tree")
	pidFile := filepath.Join(t.TempDir(), "grandchild.pid")
	spec.Env = append(spec.Env, "CLIRUN_TEST_PIDFILE="+pidFile)

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- Run(ctx, spec) }()

	var grandchild int
	deadline := time.Now().Add(15 * time.Second)
	for grandchild == 0 && time.Now().Before(deadline) {
		if data, err := os.ReadFile(pidFile); err == nil && len(data) > 0 {
			_, _ = fmt.Sscan(string(data), &grandchild)
		}
		time.Sleep(20 * time.Millisecond)
	}
	if grandchild == 0 {
		cancel()
		t.Fatal("grandchild never started")
	}

	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Errorf("Run after cancel = %v, want context.Canceled", err)
		}
	case <-time.After(20 * time.Second):
		t.Fatal("Run did not return after cancellation")
	}

	deadline = time.Now().Add(15 * time.Second)
	for processAlive(grandchild) {
		if time.Now().After(deadline) {
			t.Fatalf("grandchild %d survived cancellation of the tree", grandchild)
		}
		time.Sleep(50 * time.Millisecond)
	}
}

func TestWatchParentStopsTheChildWhenThePipeCloses(t *testing.T) {
	spec := helperSpec(t, "parent-watch")
	spec.ExitWithParent = true
	marker := filepath.Join(t.TempDir(), "marker")
	spec.Env = append(spec.Env, "CLIRUN_TEST_PIDFILE="+marker)

	// Run closes its end of the pipe only after the child exits, so the parent's death
	// is simulated by cancelling: the child must exit from either signal or EOF.
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	cmd, err := build(spec)
	if err != nil {
		t.Fatal(err)
	}
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	cmd.Stdin = r
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	_ = r.Close()
	_ = w.Close() // the parent "dies": EOF reaches the child
	waited := make(chan error, 1)
	go func() { waited <- cmd.Wait() }()
	select {
	case err := <-waited:
		if err != nil {
			t.Fatalf("child exited with %v, want a clean stop on EOF", err)
		}
	case <-ctx.Done():
		_ = cmd.Process.Kill()
		t.Fatal("child kept running after its parent pipe closed")
	}
	if data, _ := os.ReadFile(marker); string(data) != "parent gone" {
		t.Fatalf("marker = %q, want the child to observe the parent leaving", data)
	}
}
