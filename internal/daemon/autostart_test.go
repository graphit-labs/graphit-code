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
	if err := pf.Write(); err != nil {
		t.Fatalf("Write: %v", err)
	}

	started, err := EnsureRunning()
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
	if started {
		t.Error("expected started=false when daemon is already alive")
	}
}
