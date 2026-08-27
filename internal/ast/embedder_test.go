package ast

import (
	"testing"
)

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

// Which labels are embeddable is the GRAMMAR's answer, resolved per language —
// there is no list in this package to assert against. What is worth pinning here
// is the resolution itself: a language nobody declared embeds nothing, rather
// than falling back to a built-in guess that would answer for grammars the binary
// has never seen. See ExternalQueryFile.EmbedLabels.
//
// Coverage of the shipped grammars' own declarations lives in
// embed_labels_coverage_test.go.
func TestEmbedLabelsResolvePerLanguage(t *testing.T) {
	if got := EmbedLabelsForLang("", "no-such-language-anywhere"); len(got) != 0 {
		t.Errorf("undeclared language resolved to %v, want nothing", got)
	}
	if got := EmbedLabelsForLang("", ""); len(got) != 0 {
		t.Errorf("empty language resolved to %v, want nothing", got)
	}

	projectDir := stageEmbedLabelsGrammar(t, "Function", "Comment")
	got := EmbedLabelsForLang(projectDir, embedLabelsTestLang)
	if len(got) != 2 || got[0] != "Function" || got[1] != LabelComment {
		t.Errorf("EmbedLabelsForLang = %v, want [Function Comment]", got)
	}
}

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
