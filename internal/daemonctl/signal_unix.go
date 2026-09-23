//go:build !windows

package daemonctl

import (
	"os"
	"syscall"
)

func terminateProcess(proc *os.Process) error { return proc.Signal(syscall.SIGTERM) }
