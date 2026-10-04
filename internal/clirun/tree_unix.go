//go:build !windows

package clirun

import (
	"os/exec"
	"sync"
	"syscall"
	"time"
)

// processTree stops a started child and, when it leads its own group, everything the
// child started.
type processTree struct {
	cmd      *exec.Cmd
	group    bool
	finished chan struct{}
	once     sync.Once
}

// configureTree prepares cmd before Start. lead makes the child the leader of a new
// process group; a child started by a managed process stays in that process's group so
// one signal from the outermost manager reaches the whole tree.
func configureTree(cmd *exec.Cmd, lead, _ bool) *processTree {
	if lead {
		cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	}
	// Bounds Wait when a descendant keeps an output pipe open after the child exits.
	cmd.WaitDelay = terminateGrace + 5*time.Second
	return &processTree{cmd: cmd, group: lead, finished: make(chan struct{})}
}

func (t *processTree) signal(sig syscall.Signal) {
	if t.cmd.Process == nil {
		return
	}
	pid := t.cmd.Process.Pid
	if t.group {
		pid = -pid
	}
	_ = syscall.Kill(pid, sig)
}

// stop asks the tree to terminate and kills it if it is still running after the grace
// period. The kill is skipped once Wait has returned, so a recycled id is never hit.
func (t *processTree) stop() {
	t.signal(syscall.SIGTERM)
	go func() {
		select {
		case <-t.finished:
		case <-time.After(terminateGrace):
			t.signal(syscall.SIGKILL)
		}
	}()
}

func (t *processTree) done() { t.once.Do(func() { close(t.finished) }) }
