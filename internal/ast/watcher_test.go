package ast

import (
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// DefaultWatcherConfig
// ---------------------------------------------------------------------------

func TestDefaultWatcherConfig(t *testing.T) {
	cfg := DefaultWatcherConfig()

	if cfg.Debounce != 500*time.Millisecond {
		t.Errorf("expected Debounce 500ms, got %v", cfg.Debounce)
	}
	if cfg.Workers != 2 {
		t.Errorf("expected Workers 2, got %d", cfg.Workers)
	}
	if !cfg.IndexSource {
		t.Error("expected IndexSource true")
	}
	if cfg.PollInterval != 2*time.Second {
		t.Errorf("expected PollInterval 2s, got %v", cfg.PollInterval)
	}
	if cfg.Cluster != "" {
		t.Errorf("expected empty Cluster, got %q", cfg.Cluster)
	}
}

// ---------------------------------------------------------------------------
// dirtyFileMtimes (pure function test — no filesystem state needed)
// ---------------------------------------------------------------------------

func TestDirtyFileMtimes_EmptyPorcelain(t *testing.T) {
	got := dirtyFileMtimes("", "/tmp/fake")
	if got != "" {
		t.Errorf("expected empty string for empty porcelain, got %q", got)
	}
}

func TestDirtyFileMtimes_ShortLines(t *testing.T) {
	// Lines shorter than 4 chars should be skipped
	got := dirtyFileMtimes("AB\nCD\n", "/tmp/fake")
	if got != "" {
		t.Errorf("expected empty string for short lines, got %q", got)
	}
}

func TestDirtyFileMtimes_DirectorySuffix(t *testing.T) {
	// Lines ending with "/" are directories, should be skipped
	got := dirtyFileMtimes("?? somedir/\n", "/tmp/fake")
	if got != "" {
		t.Errorf("expected empty for directory suffix, got %q", got)
	}
}
