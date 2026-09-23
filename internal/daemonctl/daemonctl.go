// Package daemonctl provides lightweight daemon lifecycle helpers that
// do not import heavy packages (ast, sqlite). Safe for CGO_ENABLED=0 binaries.
package daemonctl

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

const (
	daemonReadyTimeout = 5 * time.Second
	daemonReadyPoll    = 10 * time.Millisecond
)

func DaemonDir() string {
	return filepath.Join(brand.GlobalDir(), "daemon")
}

func PIDFilePath() string  { return filepath.Join(DaemonDir(), "daemon.pid") }
func PortFilePath() string { return filepath.Join(DaemonDir(), "mcp.port") }
func KeyFilePath() string  { return filepath.Join(DaemonDir(), "mcp.key") }

func spawnLockPath() string { return filepath.Join(DaemonDir(), ".spawn.lock") }

// LogFilePath returns the daemon log.
func LogFilePath() string { return filepath.Join(DaemonDir(), "daemon.log") }

func AttachStderrToFile(cmd *exec.Cmd, path string) func() {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return func() {}
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return func() {}
	}
	cmd.Stderr = f
	return func() { _ = f.Close() }
}

// AttachLogStderr sends a spawned daemon's stderr to the daemon log.
func AttachLogStderr(cmd *exec.Cmd) func() {
	return AttachStderrToFile(cmd, LogFilePath())
}

func EnsureRunning() (bool, error) {
	if err := os.MkdirAll(DaemonDir(), 0o755); err != nil {
		return false, err
	}

	// A live but stuck spawner must not make every subsequent caller wait forever.
	spawnLock, err := lockfile.Acquire(spawnLockPath(), 2*daemonReadyTimeout)
	if err != nil {
		return false, fmt.Errorf("acquiring daemon spawn lock: %w", err)
	}
	defer spawnLock.Release()

	status, err := daemonservice.GetStatus()
	if err != nil {
		return false, fmt.Errorf("checking daemon service: %w", err)
	}
	if !status.Installed {
		if err := daemonservice.EnsureInstalled(); err != nil {
			return false, fmt.Errorf("registering daemon service: %w", err)
		}
	}
	locked, err := fileLockState(PIDFilePath())
	if err != nil {
		return false, fmt.Errorf("checking daemon lock: %w", err)
	}
	if locked && status.Active {
		return false, nil
	}
	if locked {
		if err := stopUnmanagedDaemon(); err != nil {
			return false, fmt.Errorf("handing existing daemon to service: %w", err)
		}
	}
	if err := daemonservice.Start(); err != nil {
		return false, fmt.Errorf("starting daemon service: %w", err)
	}
	if err := waitForFileLock(PIDFilePath(), daemonReadyTimeout, daemonReadyPoll); err != nil {
		return true, fmt.Errorf("waiting for managed daemon readiness: %w", err)
	}
	return true, nil
}

// Stop intentionally disables the supervisor's restart path before stopping
// any remaining foreground or legacy daemon that still holds the PID lock.
func Stop() error {
	status, err := daemonservice.GetStatus()
	if err != nil {
		return err
	}
	if status.Installed {
		if err := daemonservice.Stop(); err != nil {
			return err
		}
		if err := waitForFileUnlock(PIDFilePath(), 2*time.Second, 20*time.Millisecond); err == nil {
			return nil
		}
	}
	locked, err := fileLockState(PIDFilePath())
	if err != nil {
		return err
	}
	if locked {
		return stopUnmanagedDaemon()
	}
	return nil
}

// Restart returns the daemon to OS supervision even when it began as a
// foreground or legacy detached process.
func Restart() error {
	if err := Stop(); err != nil {
		return err
	}
	_, err := EnsureRunning()
	return err
}

func stopUnmanagedDaemon() error {
	data, err := os.ReadFile(PIDFilePath())
	if err != nil {
		return err
	}
	line := strings.TrimSpace(strings.SplitN(string(data), "\n", 2)[0])
	pid, err := strconv.Atoi(line)
	if err != nil || pid <= 0 || pid == os.Getpid() {
		return fmt.Errorf("invalid daemon PID %q", line)
	}
	proc, err := os.FindProcess(pid)
	if err != nil {
		return err
	}
	if err := terminateProcess(proc); err != nil {
		return err
	}
	return waitForFileUnlock(PIDFilePath(), daemonReadyTimeout, daemonReadyPoll)
}

func waitForFileUnlock(path string, timeout, poll time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		locked, err := fileLockState(path)
		if err != nil {
			return err
		}
		if !locked {
			return nil
		}
		if !time.Now().Before(deadline) {
			return fmt.Errorf("daemon PID lock did not release within %s", timeout)
		}
		time.Sleep(poll)
	}
}

func fileLocked(path string) bool {
	locked, _ := fileLockState(path)
	return locked
}

// IsRunning reports whether the global daemon currently holds its PID lock.
func IsRunning() bool { return fileLocked(PIDFilePath()) }

func fileLockState(path string) (bool, error) {
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		if os.IsNotExist(err) {
			return false, nil
		}
		return false, err
	}
	defer f.Close()

	if err := flockProbe(f); err != nil {
		if flockContended(err) {
			return true, nil
		}
		return false, err
	}
	flockProbeRelease(f)
	return false, nil
}

func waitForFileLock(path string, timeout, poll time.Duration) error {
	locked, err := fileLockState(path)
	if err != nil {
		return err
	}
	if locked {
		return nil
	}
	if timeout <= 0 {
		return fmt.Errorf("PID file lock was not acquired")
	}
	if poll <= 0 {
		poll = daemonReadyPoll
	}
	timer := time.NewTimer(timeout)
	defer timer.Stop()
	ticker := time.NewTicker(poll)
	defer ticker.Stop()
	for {
		select {
		case <-ticker.C:
			locked, err := fileLockState(path)
			if err != nil {
				return err
			}
			if locked {
				return nil
			}
		case <-timer.C:
			return fmt.Errorf("PID file lock was not acquired within %s", timeout)
		}
	}
}

func ResolveExe() string {
	if launcher := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); launcher != "" {
		if _, err := os.Stat(launcher); err == nil {
			return launcher
		}
	}
	exe, err := os.Executable()
	if err != nil {
		return ""
	}
	exe, err = filepath.EvalSymlinks(exe)
	if err != nil {
		return ""
	}
	return exe
}
