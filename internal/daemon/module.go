package daemon

import (
	"context"
	"sync"
	"time"
)

type ModuleState int

const (
	ModuleStopped ModuleState = iota
	ModuleRunning
	ModuleCrashed
	ModuleFailed
)

type WatchModule interface {
	Name() string

	Start(ctx context.Context) error
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
