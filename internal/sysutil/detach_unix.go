//go:build !windows

package sysutil

import (
	"os/exec"
	"syscall"
)

func DetachProcess(cmd *exec.Cmd) {
	if cmd.SysProcAttr == nil {
		cmd.SysProcAttr = &syscall.SysProcAttr{}
	}

	cmd.SysProcAttr.Setsid = true
}
