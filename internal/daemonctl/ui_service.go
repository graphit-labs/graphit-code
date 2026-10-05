package daemonctl

import (
	"context"
	"fmt"
	"sync"
	"time"
)

const UIStartPath = "/_graphit/ui/start"

// UIService waits without constructing a server. Once requested, subsequent
// supervisor generations run it again; individual callers do not own its lifetime.
type UIService struct {
	requested chan struct{}
	once      sync.Once
	run       func(context.Context) error
	url       func() string
}

func NewUIService(run func(context.Context) error) *UIService {
	return &UIService{requested: make(chan struct{}), run: run, url: PublishedUIURL}
}

func (s *UIService) Name() string { return "ui" }

func (s *UIService) Activate() { s.once.Do(func() { close(s.requested) }) }

func (s *UIService) Start(ctx context.Context) error {
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-s.requested:
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	return s.run(ctx)
}

func (s *UIService) Ensure(ctx context.Context) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	s.Activate()
	ticker := time.NewTicker(25 * time.Millisecond)
	defer ticker.Stop()
	for {
		if address := s.url(); address != "" {
			return address, nil
		}
		select {
		case <-ctx.Done():
			return "", fmt.Errorf("waiting for daemon UI: %w", ctx.Err())
		case <-ticker.C:
		}
	}
}
