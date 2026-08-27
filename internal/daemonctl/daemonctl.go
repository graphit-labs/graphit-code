// Package daemonctl provides lightweight daemon lifecycle helpers that
// do not import heavy packages (ast, sqlite). Safe for CGO_ENABLED=0 binaries.
package daemonctl

import (
	"os"
	"os/exec"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
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

// AttachStderrToFile points a spawned process's stderr at path, appending, and
// returns a closer for the parent's copy of the descriptor: the child inherits it,
// so the parent closes after Start.
//
// Spawning used to set Stderr = nil, which discards it. The Go runtime writes
// panics and SIGQUIT stack dumps to stderr, NOT through the logger, so throwing
// stderr away made every crash and every hang of the spawned process
// indiagnosable -- the log showed the last event it had chosen to record and then
// nothing, with no way to tell a deadlock from an idle process.
//
// Best effort by design: if the file cannot be opened the process is still
// spawned, because losing the daemon is worse than losing its stderr.
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

	sf, lockErr := os.OpenFile(spawnLockPath(), os.O_CREATE|os.O_RDWR, 0o600)
	if lockErr == nil {
		if err := flockExclusiveBlocking(sf); err != nil {
			_ = sf.Close()
			sf = nil
		}
	}
	if sf != nil {
		defer func() {
			flockProbeRelease(sf)
			_ = sf.Close()
		}()
	}

	if isDaemonLocked() {
		return false, nil
	}

	exe := ResolveExe()
	if exe == "" {
		return false, nil
	}

	cmd := exec.Command(exe, "daemon")
	cmd.Stdin = nil
	cmd.Stdout = nil
	closeLog := AttachLogStderr(cmd)
	defer closeLog()
	sysutil.DetachProcess(cmd)

	if err := cmd.Start(); err != nil {
		return false, err
	}
	go func() { _ = cmd.Wait() }()
	return true, nil
}

func isDaemonLocked() bool {
	f, err := os.Open(PIDFilePath())
	if err != nil {
		return false
	}
	defer f.Close()

	if err := flockProbe(f); err != nil {
		return true
	}
	flockProbeRelease(f)
	return false
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
