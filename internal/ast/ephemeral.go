package ast

import (
	"context"
	"sync"
)

type EphemeralDB struct {
	cfg LadybugConfig

	mu      sync.Mutex
	backend *LadybugBackend
	closed  bool
}

func NewEphemeralDB(cfg LadybugConfig) *EphemeralDB {
	return &EphemeralDB{cfg: cfg}
}

func (e *EphemeralDB) acquire() (*LadybugBackend, error) {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return nil, context.Canceled
	}

	if e.backend == nil {
		e.backend = NewLadybugDB(e.cfg)
	}

	return e.backend, nil
}

func (e *EphemeralDB) Release() {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.closeLocked()
}

func (e *EphemeralDB) closeLocked() {
	if e.backend != nil {
		_ = e.backend.Shutdown()
		_ = e.backend.Close()
		e.backend = nil
	}
}

func (e *EphemeralDB) Query(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error) {
	b, err := e.acquire()
	if err != nil {
		return nil, err
	}
	return b.Query(ctx, cypher, params)
}

func (e *EphemeralDB) Execute(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error) {
	b, err := e.acquire()
	if err != nil {
		return nil, err
	}
	return b.Execute(ctx, cypher, params)
}

func (e *EphemeralDB) ExecuteBatch(ctx context.Context, queries []BatchQuery) error {
	b, err := e.acquire()
	if err != nil {
		return err
	}
	return b.ExecuteBatch(ctx, queries)
}

func (e *EphemeralDB) Ping(ctx context.Context) error {
	b, err := e.acquire()
	if err != nil {
		return err
	}
	return b.Ping(ctx)
}

func (e *EphemeralDB) BackendType() string { return "ladybug-ephemeral" }

func (e *EphemeralDB) Close() error {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.closed = true
	e.closeLocked()
	return nil
}
