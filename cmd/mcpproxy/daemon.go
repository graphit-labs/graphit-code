package main

import (
	"os/exec"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func ensureDaemonRunning() {
	exe := resolveDaemonExe()
	if exe == "" {
		return
	}

	cmd := exec.Command(exe, "daemon")
	cmd.Stdin = nil
	cmd.Stdout = nil
	cmd.Stderr = nil
	sysutil.DetachProcess(cmd)

	if err := cmd.Start(); err != nil {
		return
	}
	_ = cmd.Process.Release()
}
