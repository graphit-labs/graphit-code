package tray

import (
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

type snapshot struct {
	state    string
	activity string
	running  bool
	canStart bool
	canStop  bool
	service  daemonservice.Status
	err      error
}

func capture(now time.Time) snapshot {
	service, err := daemonservice.GetStatus()
	if err != nil {
		return snapshot{state: "Service error", activity: short(err.Error(), 90), err: err}
	}
	running, err := daemonservice.IsRunning()
	if err != nil {
		return snapshot{state: "Daemon status error", activity: short(err.Error(), 90), err: err}
	}
	return describe(service, running, recentActivity(daemonctl.LogFilePath(), now))
}

func describe(service daemonservice.Status, running bool, activity string) snapshot {
	s := snapshot{service: service, running: running, canStart: !running, canStop: running || service.Active, activity: activity}
	switch {
	case running && service.Active:
		s.state = "Daemon running"
	case running:
		s.state = "Daemon running in foreground"
	case service.Active:
		s.state = "Daemon starting or restarting"
	case !service.Installed:
		s.state = "Daemon stopped; service not installed"
	default:
		s.state = "Daemon stopped"
	}
	return s
}

func recentActivity(path string, now time.Time) string {
	f, err := os.Open(path)
	if err != nil {
		return "No recorded activity"
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil || info.Size() == 0 {
		return "No recorded activity"
	}
	const maxTail = 16 * 1024
	start := info.Size() - maxTail
	if start < 0 {
		start = 0
	}
	if _, err := f.Seek(start, io.SeekStart); err != nil {
		return "No recorded activity"
	}
	data, err := io.ReadAll(io.LimitReader(f, maxTail))
	if err != nil {
		return "No recorded activity"
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		line := strings.TrimSpace(lines[i])
		if len(line) < 22 || line[0] != '[' || line[20] != ']' {
			continue
		}
		at, err := time.ParseInLocation("2006-01-02 15:04:05", line[1:20], time.Local)
		if err != nil {
			continue
		}
		message := strings.TrimSpace(line[21:])
		if message == "" {
			continue
		}
		return fmt.Sprintf("Last event %s ago: %s", age(now.Sub(at)), short(message, 70))
	}
	return "No recorded activity"
}

func age(d time.Duration) string {
	if d < 0 {
		d = 0
	}
	switch {
	case d < time.Minute:
		return "<1m"
	case d < time.Hour:
		return fmt.Sprintf("%dm", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%dh", int(d.Hours()))
	default:
		return fmt.Sprintf("%dd", int(d.Hours()/24))
	}
}

func short(s string, max int) string {
	runes := []rune(strings.Join(strings.Fields(s), " "))
	if len(runes) <= max {
		return string(runes)
	}
	return string(runes[:max-1]) + "…"
}
