package daemon

import (
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// EmbeddingModule — construction and name
// ---------------------------------------------------------------------------

func TestNewEmbeddingModule_DefaultInterval(t *testing.T) {
	m := NewEmbeddingModule("/root", 0, "/cache")
	if m.interval != 2*time.Minute {
		t.Errorf("expected default interval 2m, got %s", m.interval)
	}
	if m.rootPath != "/root" {
		t.Errorf("expected rootPath '/root', got %q", m.rootPath)
	}
	if m.cacheDir != "/cache" {
		t.Errorf("expected cacheDir '/cache', got %q", m.cacheDir)
	}
}

func TestNewEmbeddingModule_CustomInterval(t *testing.T) {
	m := NewEmbeddingModule("/root", 5*time.Minute, "/cache")
	if m.interval != 5*time.Minute {
		t.Errorf("expected 5m, got %s", m.interval)
	}
}

func TestNewEmbeddingModule_NegativeInterval(t *testing.T) {
	m := NewEmbeddingModule("/root", -1*time.Second, "/cache")
	if m.interval != 2*time.Minute {
		t.Errorf("negative interval should default to 2m, got %s", m.interval)
	}
}

func TestEmbeddingModule_Name(t *testing.T) {
	m := NewEmbeddingModule("/root", 0, "")
	if m.Name() != "embedding" {
		t.Errorf("expected 'embedding', got %q", m.Name())
	}
}

// ---------------------------------------------------------------------------
// DreamModule — construction and name
// ---------------------------------------------------------------------------

func TestNewDreamModule(t *testing.T) {
	m := NewDreamModule("/project", "vscode")
	if m.projectDir != "/project" {
		t.Errorf("expected '/project', got %q", m.projectDir)
	}
	if m.ide != "vscode" {
		t.Errorf("expected 'vscode', got %q", m.ide)
	}
}

func TestDreamModule_Name(t *testing.T) {
	m := NewDreamModule("", "")
	if m.Name() != "dream" {
		t.Errorf("expected 'dream', got %q", m.Name())
	}
}
