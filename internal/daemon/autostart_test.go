package daemon

import (
	"os"
	"testing"
)

func TestEnsureRunning_AlreadyAlive(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	pf := NewPIDFile()
	// Use Acquire to hold flock — this simulates a running daemon.
	if err := pf.Acquire(); err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	defer pf.Release()

	started, err := EnsureRunning()
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
	if started {
		t.Error("expected started=false when daemon is already alive")
	}
}
