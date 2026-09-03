//go:build !windows

package sysutil

import (
	"os/exec"
	"syscall"
	"testing"
)

func TestDetachProcess(t *testing.T) {
	cmd1 := exec.Command("echo", "test")
	cmd1.SysProcAttr = nil

	DetachProcess(cmd1)

	if cmd1.SysProcAttr == nil {
		t.Fatal("expected SysProcAttr to be initialized, got nil")
	}
	if !cmd1.SysProcAttr.Setsid {
		t.Error("expected Setsid to be true")
	}

	cmd2 := exec.Command("echo", "test")
	cmd2.SysProcAttr = &syscall.SysProcAttr{
		Setsid: false,
	}

	DetachProcess(cmd2)

	if !cmd2.SysProcAttr.Setsid {
		t.Error("expected Setsid to be true")
	}
}
