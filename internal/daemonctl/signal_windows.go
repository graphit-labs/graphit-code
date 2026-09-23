//go:build windows

package daemonctl

import "os"

func terminateProcess(proc *os.Process) error { return proc.Kill() }
