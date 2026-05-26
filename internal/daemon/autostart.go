package daemon

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func EnsureRunning() (started bool, err error) {
	pid := NewPIDFile()
	if pid.IsAlive() != nil {
		return false, nil
	}

	exe, err := os.Executable()
	if err != nil {
		return false, fmt.Errorf("finding executable: %w", err)
	}
	exe, err = filepath.EvalSymlinks(exe)
	if err != nil {
		return false, fmt.Errorf("resolving executable symlink: %w", err)
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
