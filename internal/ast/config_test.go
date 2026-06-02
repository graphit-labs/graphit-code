package ast

import (
	"testing"
)

// ---------------------------------------------------------------------------
// sanitizeContextName
// ---------------------------------------------------------------------------

func TestSanitizeContextName(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"My Project", "my-project"},
		{"  UPPERCASE  ", "uppercase"},
		{"with spaces", "with-spaces"},
		{"special@chars!", "specialchars"},
		{"valid-name_123", "valid-name_123"},
		{"MixedCase", "mixedcase"},
		{"", "unnamed"},
		{"   ", "unnamed"},
		{"!!!###", "unnamed"},
		{"hello.world", "helloworld"},
		{"path/to/project", "pathtoproject"},
	}
	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := sanitizeContextName(tt.input)
			if got != tt.want {
				t.Errorf("sanitizeContextName(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// envOr
// ---------------------------------------------------------------------------

func TestEnvOr(t *testing.T) {
	t.Run("returns_fallback_for_unset", func(t *testing.T) {
		got := envOr("GRAPHIT_TEST_NONEXISTENT_KEY_XYZ_12345", "fallback")
		if got != "fallback" {
			t.Errorf("expected 'fallback', got %q", got)
		}
	})

	t.Run("returns_env_if_set", func(t *testing.T) {
		// PATH is always set on all platforms
		got := envOr("PATH", "fallback")
		if got == "fallback" {
			t.Error("expected PATH value, got fallback")
		}
	})
}

// ---------------------------------------------------------------------------
// DefaultConfig
// ---------------------------------------------------------------------------

func TestDefaultConfig(t *testing.T) {
	cfg := DefaultConfig()
	if cfg == nil {
		t.Fatal("expected non-nil config")
	}
	if cfg.Contexts == nil {
		t.Error("expected non-nil Contexts map")
	}
	if cfg.ImportedContexts == nil {
		t.Error("expected non-nil ImportedContexts map")
	}
	if cfg.OpenAIModel != "gpt-4o-mini" {
		t.Errorf("expected default model 'gpt-4o-mini', got %q", cfg.OpenAIModel)
	}
	if cfg.LadybugPath == "" {
		t.Error("expected non-empty LadybugPath")
	}
}
