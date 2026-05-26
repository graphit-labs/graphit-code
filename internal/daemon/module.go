package daemon

import (
	"context"
	"fmt"
	"sync"
	"time"
)

type ModuleState int

const (
	ModuleStopped ModuleState = iota
	ModuleRunning
	ModuleCrashed
	ModuleFailed
	ModuleDisabled
)

func (s ModuleState) String() string {
	switch s {
	case ModuleStopped:
		return "stopped"
	case ModuleRunning:
		return "running"
	case ModuleCrashed:
		return "crashed"
	case ModuleFailed:
		return "failed"
	case ModuleDisabled:
		return "disabled"
	default:
		return "unknown"
	}
}

type WatchModule interface {
	Name() string

	Start(ctx context.Context) error
}

type ModuleStatus struct {
	Name      string
	State     ModuleState
	StartedAt time.Time
	LastError string
	Restarts  int
}

type moduleEntry struct {
	mod WatchModule

	mu        sync.Mutex
	state     ModuleState
	startedAt time.Time
	lastError string
	restarts  int
	cancel    context.CancelFunc
}

func newModuleEntry(mod WatchModule) *moduleEntry {
	return &moduleEntry{
		mod:   mod,
		state: ModuleStopped,
	}
}

func (e *moduleEntry) status() ModuleStatus {
	e.mu.Lock()
	defer e.mu.Unlock()
	return ModuleStatus{
		Name:      e.mod.Name(),
		State:     e.state,
		StartedAt: e.startedAt,
		LastError: e.lastError,
		Restarts:  e.restarts,
	}
}

func (e *moduleEntry) setState(s ModuleState) {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.state = s
}

func (e *moduleEntry) setError(err error) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if err != nil {
		e.lastError = err.Error()
	}
}

func (e *moduleEntry) setStarted() {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.state = ModuleRunning
	e.startedAt = time.Now()
}

func (e *moduleEntry) incRestarts() int {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.restarts++
	return e.restarts
}

func (e *moduleEntry) resetRestarts() {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.restarts = 0
}

func (e *moduleEntry) String() string {
	s := e.status()
	if s.LastError != "" {
		return fmt.Sprintf("%s: %s (restarts=%d, last_error=%s)", s.Name, s.State, s.Restarts, s.LastError)
	}
	return fmt.Sprintf("%s: %s (restarts=%d)", s.Name, s.State, s.Restarts)
}
