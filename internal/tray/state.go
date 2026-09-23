package tray

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

type snapshot struct {
	state    string
	running  bool
	canStart bool
	canStop  bool
	service  daemonservice.Status
	err      error
}

func capture() snapshot {
	service, err := daemonservice.GetStatus()
	if err != nil {
		return snapshot{state: "Service error", err: err}
	}
	running, err := daemonservice.IsRunning()
	if err != nil {
		return snapshot{state: "Daemon status error", err: err}
	}
	return describe(service, running)
}

func describe(service daemonservice.Status, running bool) snapshot {
	s := snapshot{service: service, running: running, canStart: !running, canStop: running || service.Active}
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

func short(s string, max int) string {
	runes := []rune(strings.Join(strings.Fields(s), " "))
	if len(runes) <= max {
		return string(runes)
	}
	return string(runes[:max-1]) + "…"
}
