package ast

import (
	"testing"
)

// ---------------------------------------------------------------------------
// DefaultEmbeddingConfig
// ---------------------------------------------------------------------------

func TestDefaultEmbeddingConfig(t *testing.T) {
	cfg := DefaultEmbeddingConfig()
	if cfg.BatchSize != 128 {
		t.Errorf("expected BatchSize 128, got %d", cfg.BatchSize)
	}
	if cfg.MaxSourceChars != 500 {
		t.Errorf("expected MaxSourceChars 500, got %d", cfg.MaxSourceChars)
	}
	if cfg.OnProgress != nil {
		t.Error("expected nil OnProgress callback")
	}
	if cfg.EmbCache != nil {
		t.Error("expected nil EmbCache")
	}
	if cfg.ParseCache != nil {
		t.Error("expected nil ParseCache")
	}
}

// ---------------------------------------------------------------------------
// embeddableLabels
// ---------------------------------------------------------------------------

func TestEmbeddableLabels(t *testing.T) {
	if len(embeddableLabels) == 0 {
		t.Fatal("expected non-empty embeddableLabels")
	}

	expected := map[string]bool{
		"Function":  true,
		"Class":     true,
		"Method":    true,
		"Struct":    true,
		"Interface": true,
		"Module":    true,
		"Variable":  true,
		"Constant":  true,
	}

	found := make(map[string]bool)
	for _, label := range embeddableLabels {
		found[label] = true
		if label == "" {
			t.Error("found empty label in embeddableLabels")
		}
	}

	for key := range expected {
		if !found[key] {
			t.Errorf("expected %q in embeddableLabels", key)
		}
	}
}

// ---------------------------------------------------------------------------
// NewEmbedder
// ---------------------------------------------------------------------------

func TestNewEmbedder_NilClient(t *testing.T) {
	cfg := DefaultEmbeddingConfig()
	e := NewEmbedder(nil, cfg)
	if e == nil {
		t.Fatal("expected non-nil embedder")
	}
	if e.cfg.BatchSize != 128 {
		t.Errorf("expected BatchSize 128, got %d", e.cfg.BatchSize)
	}
}
