package daemon

import (
	"os/exec"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
)

func EnsureRunning() (started bool, err error) {
	return daemonctl.EnsureRunning()
}

// AttachLogStderr sends a spawned daemon's stderr to the daemon log.
func AttachLogStderr(cmd *exec.Cmd) func() { return daemonctl.AttachLogStderr(cmd) }

// AttachStderrToFile sends a spawned process's stderr to path.
func AttachStderrToFile(cmd *exec.Cmd, path string) func() {
	return daemonctl.AttachStderrToFile(cmd, path)
}
