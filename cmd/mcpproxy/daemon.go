package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func ensureDaemonRunning() {
	if isDaemonAlive() {
		return
	}

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
	go func() { _ = cmd.Wait() }() // reap child to avoid zombie
}

func isDaemonAlive() bool {
	pidPath := filepath.Join(brand.GlobalDir(), "daemon", "daemon.pid")
	data, err := os.ReadFile(pidPath)
	if err != nil {
		return false
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) < 1 {
		return false
	}
	pid, err := strconv.Atoi(strings.TrimSpace(lines[0]))
	if err != nil {
		return false
	}
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	return proc.Signal(syscall.Signal(0)) == nil
}

