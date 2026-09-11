//go:build darwin

package daemon

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

func InstallScheduler() error {
	exe, err := resolveExePath()
	if err != nil {
		return err
	}

	label := schedulerLabel()
	plistDir := launchAgentsDir()
	plistPath := filepath.Join(plistDir, label+".plist")

	if err := os.MkdirAll(plistDir, 0o755); err != nil {
		return fmt.Errorf("creating LaunchAgents dir: %w", err)
	}

	_ = exec.Command("launchctl", "unload", plistPath).Run()

	plistContent := fmt.Sprintf(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>%s</string>
    <key>ProgramArguments</key>
    <array>
        <string>%s</string>
        <string>daemon</string>
    </array>
    <key>StartInterval</key>
    <integer>60</integer>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/dev/null</string>
    <key>StandardErrorPath</key>
    <string>/dev/null</string>
</dict>
</plist>`, label, exe)

	if err := os.WriteFile(plistPath, []byte(plistContent), 0o644); err != nil {
		return fmt.Errorf("writing plist: %w", err)
	}

	cmd := exec.Command("launchctl", "load", plistPath)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("loading LaunchAgent: %w", err)
	}
	return nil
}

func RemoveScheduler() error {
	label := schedulerLabel()
	plistPath := filepath.Join(launchAgentsDir(), label+".plist")

	_ = exec.Command("launchctl", "unload", plistPath).Run()

	if err := os.Remove(plistPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("removing plist: %w", err)
	}
	return nil
}

func SchedulerStatus() string {
	label := schedulerLabel()
	plistPath := filepath.Join(launchAgentsDir(), label+".plist")

	if _, err := os.Stat(plistPath); os.IsNotExist(err) {
		return "not installed (no LaunchAgent plist)"
	}

	out, err := exec.Command("launchctl", "list", label).Output()
	if err == nil && len(out) > 0 {
		return "installed and loaded (macOS LaunchAgent)"
	}
	return "installed but not loaded (macOS LaunchAgent)"
}

func launchAgentsDir() string {
	home, _ := os.UserHomeDir()
	return filepath.Join(home, "Library", "LaunchAgents")
}
