package daemon

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func EnsureRunning() (started bool, err error) {
	pid := NewPIDFile()
	if pid.IsAlive() != nil {
		return false, nil
	}

	exe := resolveDaemonExe()
	if exe == "" {
		return false, fmt.Errorf("cannot determine executable path")
	}

	cmd := exec.Command(exe, "daemon")
	cmd.Stdin = nil
	cmd.Stdout = nil
	cmd.Stderr = nil

	sysutil.DetachProcess(cmd)

	if err := cmd.Start(); err != nil {
		return false, fmt.Errorf("spawning daemon: %w", err)
	}

	_ = cmd.Process.Release()

	return true, nil
}

// resolveDaemonExe returns the best executable path for spawning a new daemon.
// It prefers the launcher binary (set by the launcher via GRAPHIT_LAUNCHER_PATH)
// because during a version upgrade the old graphit-core binary may have been
// deleted. The launcher extracts the new runtime and delegates to the correct
// graphit-core. Falls back to os.Executable() for non-launcher installations.
func resolveDaemonExe() string {
	if launcher := os.Getenv(brand.EnvVar("LAUNCHER_PATH")); launcher != "" {
		if _, err := os.Stat(launcher); err == nil {
			return launcher
		}
	}

	// Fallback: use the currently running binary (works for direct installs
	// and for non-upgrade EnsureRunning calls from CLI/MCP).
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
