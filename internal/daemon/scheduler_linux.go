//go:build linux

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

	marker := cronMarker()
	cronLine := fmt.Sprintf("* * * * * %s daemon > /dev/null 2>&1", exe)
	newEntry := marker + "\n" + cronLine

	existing := ""
	out, err := exec.Command("crontab", "-l").Output()
	if err == nil {
		existing = string(out)
	}

	cleaned := removeCronEntry(existing, marker)

	final := strings.TrimRight(cleaned, "\n")
	if final != "" {
		final += "\n"
	}
	final += newEntry + "\n"

	cmd := exec.Command("crontab", "-")
	cmd.Stdin = strings.NewReader(final)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("writing crontab: %w", err)
	}
	return nil
}

func RemoveScheduler() error {
	marker := cronMarker()

	out, err := exec.Command("crontab", "-l").Output()
	if err != nil {

		return nil
	}

	cleaned := removeCronEntry(string(out), marker)
	cleaned = strings.TrimRight(cleaned, "\n")

	if strings.TrimSpace(cleaned) == "" {

		_ = exec.Command("crontab", "-r").Run()
		return nil
	}

	cmd := exec.Command("crontab", "-")
	cmd.Stdin = strings.NewReader(cleaned + "\n")
	return cmd.Run()
}

func SchedulerStatus() string {
	marker := cronMarker()

	out, err := exec.Command("crontab", "-l").Output()
	if err != nil {
		return "not installed (no user crontab)"
	}

	if strings.Contains(string(out), marker) {
		return "installed (user crontab, every 1 minute)"
	}
	return "not installed"
}

func removeCronEntry(crontab, marker string) string {
	lines := strings.Split(crontab, "\n")
	var result []string
	skip := false
	for _, line := range lines {
		if strings.TrimSpace(line) == marker {
			skip = true
			continue
		}
		if skip {
			skip = false
			continue
		}
		result = append(result, line)
	}
	return strings.Join(result, "\n")
}
