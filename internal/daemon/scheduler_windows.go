//go:build windows

package daemon

import (
	"fmt"
	"os/exec"
	"strings"
)

func InstallScheduler() error {
	exe, err := resolveExePath()
	if err != nil {
		return err
	}

	taskName := schedulerTaskName()
	fullCmd := fmt.Sprintf(`"%s" daemon`, exe)

	_ = exec.Command("schtasks", "/delete", "/tn", taskName, "/f").Run()

	cmd := exec.Command("schtasks", "/create",
		"/tn", taskName,
		"/tr", fullCmd,
		"/sc", "minute",
		"/mo", "1",
		"/f",
	)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("creating scheduled task: %w", err)
	}
	return nil
}

func RemoveScheduler() error {
	taskName := schedulerTaskName()

	_ = exec.Command("schtasks", "/delete", "/tn", taskName, "/f").Run()
	return nil
}

func SchedulerStatus() string {
	taskName := schedulerTaskName()
	out, err := exec.Command("schtasks", "/query", "/tn", taskName).Output()
	if err != nil {
		return "not installed (no scheduled task)"
	}
	if strings.Contains(string(out), taskName) {
		return "installed (Windows Task Scheduler, every 1 minute)"
	}
	return "not installed"
}

func schedulerTaskName() string {

	label := schedulerLabel()
	return strings.ReplaceAll(label, ".", "_") + "_watchdog"
}
