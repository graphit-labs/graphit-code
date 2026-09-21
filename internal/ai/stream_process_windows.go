package ai

import (
	"fmt"
	"os/exec"
)

func configureStreamProcess(cmd *exec.Cmd) {
	cmd.Cancel = func() error {
		// /T terminates tools spawned by the CLI as well as the root process.
		return exec.Command("taskkill", "/PID", fmt.Sprint(cmd.Process.Pid), "/T", "/F").Run()
	}
}
