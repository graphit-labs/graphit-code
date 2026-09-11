package daemonctl

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestStderrHelperProcess(t *testing.T) {
	if os.Getenv("DAEMONCTL_STDERR_HELPER") == "" {
		t.Skip("not the helper subprocess")
	}
	if _, err := os.Stderr.WriteString("goroutine 1 [running]: pretend stack dump\n"); err != nil {
		os.Exit(2)
	}
	os.Exit(0)
}

func TestLogFilePathIsInTheDaemonDir(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if got, want := LogFilePath(), filepath.Join(DaemonDir(), "daemon.log"); got != want {
		t.Errorf("LogFilePath() = %q, want %q", got, want)
	}
}

// A spawned process must inherit a descriptor pointing at the log, and the log
// must be appended to rather than replaced.
func TestAttachStderrToFileAppendsChildStderr(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	logPath := LogFilePath()
	if err := os.MkdirAll(filepath.Dir(logPath), 0o755); err != nil {
		t.Fatal(err)
	}
	const earlier = "[earlier] daemon started\n"
	if err := os.WriteFile(logPath, []byte(earlier), 0o644); err != nil {
		t.Fatal(err)
	}

	cmd := exec.Command(os.Args[0], "-test.run", "TestStderrHelperProcess")
	cmd.Env = append(os.Environ(), "DAEMONCTL_STDERR_HELPER=1", "HOME="+home)

	closeLog := AttachLogStderr(cmd)
	if cmd.Stderr == nil {
		closeLog()
		t.Fatal("stderr was not attached: a spawned daemon crash would be lost")
	}
	if err := cmd.Start(); err != nil {
		closeLog()
		t.Fatalf("start helper: %v", err)
	}
	closeLog()
	if err := cmd.Wait(); err != nil {
		t.Fatalf("helper exited badly: %v", err)
	}

	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	got := string(data)
	if !strings.Contains(got, "pretend stack dump") {
		t.Errorf("child stderr never reached the log; log holds %q", got)
	}
	if !strings.Contains(got, earlier) {
		t.Error("the log was truncated: stderr must be appended, not overwrite the history")
	}
}

// Losing the daemon is worse than losing its stderr, so a log that cannot be
// opened must leave the spawn untouched.
func TestAttachStderrToFileIsBestEffort(t *testing.T) {
	dir := t.TempDir()
	blocker := filepath.Join(dir, "a-file-not-a-dir")
	if err := os.WriteFile(blocker, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}

	cmd := exec.Command(os.Args[0], "-test.run", "TestNoSuchTestExists")
	closeLog := AttachStderrToFile(cmd, filepath.Join(blocker, "sub", "daemon.log"))
	defer closeLog()

	if cmd.Stderr != nil {
		t.Error("stderr should stay unset when the log cannot be opened")
	}
	if err := cmd.Start(); err != nil {
		t.Errorf("the process must still spawn: %v", err)
		return
	}
	_ = cmd.Wait()
}
