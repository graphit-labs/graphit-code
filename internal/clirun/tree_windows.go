//go:build windows

package clirun

import (
	"fmt"
	"os/exec"
	"syscall"
	"time"

	"golang.org/x/sys/windows"
)

const createNoWindow = 0x08000000

var procGetConsoleWindow = windows.NewLazySystemDLL("kernel32.dll").NewProc("GetConsoleWindow")

// hasConsole reports whether this process is attached to a console window.
func hasConsole() bool {
	if procGetConsoleWindow.Find() != nil {
		return false
	}
	hwnd, _, _ := procGetConsoleWindow.Call()
	return hwnd != 0
}

// processTree stops a started child together with every process it started.
type processTree struct {
	cmd *exec.Cmd
}

// configureTree prepares cmd before Start. A child gets no console window of its own
// unless it shares this process's console: a child that inherits the streams of a
// windowless process (a daemon-started sync) would otherwise open a new console. Windows
// has no process groups to lead, so lead is unused: taskkill /T walks the parent links.
func configureTree(cmd *exec.Cmd, _ bool, inherit bool) *processTree {
	if !inherit || !hasConsole() {
		cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: createNoWindow}
	}
	cmd.WaitDelay = terminateGrace + 5*time.Second
	return &processTree{cmd: cmd}
}

// stop ends the child and its descendants. Windows offers no portable polite stop for a
// windowless process, so the tree is terminated outright, as the agent CLI streams are.
func (t *processTree) stop() {
	if t.cmd.Process == nil {
		return
	}
	kill := exec.Command("taskkill", "/PID", fmt.Sprint(t.cmd.Process.Pid), "/T", "/F")
	kill.SysProcAttr = &syscall.SysProcAttr{CreationFlags: createNoWindow}
	if err := kill.Run(); err != nil {
		_ = t.cmd.Process.Kill()
	}
}

func (t *processTree) done() {}
