package daemonctl

import (
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// UIURLFilePath stores the address actually selected by the daemon UI module.
func UIURLFilePath() string { return filepath.Join(DaemonDir(), "ui.url") }

// PublishUIURL records the daemon PID alongside the selected URL. The PID lets
// readers ignore a file left behind by a crash or a replaced daemon.
func PublishUIURL(address string) (func(), error) {
	if err := os.MkdirAll(DaemonDir(), 0o700); err != nil {
		return nil, err
	}
	content := []byte(fmt.Sprintf("%d\n%s\n", os.Getpid(), address))
	path := UIURLFilePath()
	if err := os.WriteFile(path, content, 0o600); err != nil {
		return nil, err
	}
	return func() {
		current, err := os.ReadFile(path)
		if err == nil && string(current) == string(content) {
			_ = os.Remove(path)
		}
	}, nil
}

// PublishedUIURL returns only the URL published by the daemon holding the PID
// lock. A stale URL never sends the tray to an old or unrelated listener.
func PublishedUIURL() string {
	if !IsRunning() {
		return ""
	}
	currentPID, err := os.ReadFile(PIDFilePath())
	if err != nil {
		return ""
	}
	parts := strings.SplitN(strings.TrimSpace(string(currentPID)), "\n", 2)
	pid, err := strconv.Atoi(parts[0])
	if err != nil || pid <= 0 {
		return ""
	}
	data, err := os.ReadFile(UIURLFilePath())
	if err != nil {
		return ""
	}
	lines := strings.SplitN(strings.TrimSpace(string(data)), "\n", 2)
	if len(lines) != 2 || lines[0] != strconv.Itoa(pid) {
		return ""
	}
	parsed, err := url.Parse(strings.TrimSpace(lines[1]))
	if err != nil || parsed.Scheme != "http" || parsed.Host == "" || parsed.User != nil {
		return ""
	}
	return parsed.String()
}
