package tray

import (
	"errors"
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
	if err != nil && !errors.Is(err, daemonservice.ErrManagerUnavailable) {
		return snapshot{state: "Service error", err: err}
	}
	managerUnavailable := errors.Is(err, daemonservice.ErrManagerUnavailable)
	running, err := daemonservice.IsRunning()
	if err != nil {
		return snapshot{state: "Daemon status error", err: err}
	}
	if managerUnavailable {
		return describeUnavailable(running)
	}
	return describe(service, running)
}

func describeUnavailable(running bool) snapshot {
	if running {
		return snapshot{state: "Daemon running directly", running: true, canStop: true}
	}
	return snapshot{state: "Daemon stopped; service unavailable", canStart: true}
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
