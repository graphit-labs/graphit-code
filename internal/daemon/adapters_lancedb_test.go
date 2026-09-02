//go:build lancedb

package daemon

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestMemoryMaintenanceModuleRunsImmediatelyAndOwnsItsTicker(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	module := NewMemoryMaintenanceModule("memory-table", time.Millisecond)
	calls := make(chan string, 2)
	module.maintain = func(_ context.Context, uri string) error {
		calls <- uri
		return nil
	}
	done := make(chan error, 1)
	go func() { done <- module.Start(ctx) }()

	for range 2 {
		select {
		case got := <-calls:
			if got != "memory-table" {
				t.Fatalf("maintenance URI = %q, want memory-table", got)
			}
		case <-time.After(time.Second):
			t.Fatal("maintenance did not run")
		}
	}
	cancel()
	if err := <-done; err != nil {
		t.Fatalf("Start returned after cancellation: %v", err)
	}
}

func TestMemoryMaintenanceModuleReportsMaintenanceFailure(t *testing.T) {
	want := errors.New("compact failed")
	module := NewMemoryMaintenanceModule("memory-table", time.Hour)
	module.maintain = func(context.Context, string) error { return want }
	if got := module.Start(context.Background()); !errors.Is(got, want) {
		t.Fatalf("Start error = %v, want %v", got, want)
	}
}
