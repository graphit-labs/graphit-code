package config

import (
	"testing"
)

func TestParseGrammarOverrides(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected map[string]string
	}{
		{
			name:     "single override",
			input:    ".sql=antlr-plsql",
			expected: map[string]string{".sql": "antlr-plsql"},
		},
		{
			name:  "multiple overrides",
			input: ".sql=antlr-plsql,.pks=antlr-plsql,.ts=tree-sitter-typescript",
			expected: map[string]string{
				".sql": "antlr-plsql",
				".pks": "antlr-plsql",
				".ts":  "tree-sitter-typescript",
			},
		},
		{
			name:     "without dot prefix adds it",
			input:    "sql=antlr-plsql",
			expected: map[string]string{".sql": "antlr-plsql"},
		},
		{
			name:     "uppercase ext lowered",
			input:    ".SQL=antlr-plsql",
			expected: map[string]string{".sql": "antlr-plsql"},
		},
		{
			name:     "empty string",
			input:    "",
			expected: map[string]string{},
		},
		{
			name:     "malformed pair skipped",
			input:    ".sql=antlr-plsql,bad,.ts=tree-sitter-ts",
			expected: map[string]string{".sql": "antlr-plsql", ".ts": "tree-sitter-ts"},
		},
		{
			name:     "whitespace trimmed",
			input:    " .sql = antlr-plsql , .ts = tree-sitter-ts ",
			expected: map[string]string{".sql": "antlr-plsql", ".ts": "tree-sitter-ts"},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			result := ParseGrammarOverrides(tc.input)
			if len(result) != len(tc.expected) {
				t.Fatalf("expected %d entries, got %d: %v", len(tc.expected), len(result), result)
			}
			for k, v := range tc.expected {
				if got := result[k]; got != v {
					t.Errorf("key %q: expected %q, got %q", k, v, got)
				}
			}
		})
	}
}

func TestMergeGrammarOverrides(t *testing.T) {
	base := map[string]string{".sql": "antlr-plsql", ".py": "tree-sitter-python"}
	overlay := map[string]string{".sql": "tree-sitter-sql", ".ts": "tree-sitter-typescript"}

	result := MergeGrammarOverrides(base, overlay)

	// overlay wins for .sql
	if result[".sql"] != "tree-sitter-sql" {
		t.Errorf(".sql: expected tree-sitter-sql, got %q", result[".sql"])
	}
	// base kept for .py
	if result[".py"] != "tree-sitter-python" {
		t.Errorf(".py: expected tree-sitter-python, got %q", result[".py"])
	}
	// overlay adds .ts
	if result[".ts"] != "tree-sitter-typescript" {
		t.Errorf(".ts: expected tree-sitter-typescript, got %q", result[".ts"])
	}

	// original maps not mutated
	if base[".sql"] != "antlr-plsql" {
		t.Error("base map was mutated")
	}
}

func TestMergeGrammarOverrides_NilInputs(t *testing.T) {
	result := MergeGrammarOverrides(nil, nil)
	if result != nil {
		t.Errorf("expected nil, got %v", result)
	}

	overlay := map[string]string{".sql": "antlr-plsql"}
	result = MergeGrammarOverrides(nil, overlay)
	if result[".sql"] != "antlr-plsql" {
		t.Error("expected overlay to be returned when base is nil")
	}

	base := map[string]string{".sql": "antlr-plsql"}
	result = MergeGrammarOverrides(base, nil)
	if result[".sql"] != "antlr-plsql" {
		t.Error("expected base to be returned when overlay is nil")
	}
}

func TestResolveGrammarOverrides(t *testing.T) {
	// No config = nil
	result := ResolveGrammarOverrides(nil, nil)
	if result != nil {
		t.Errorf("expected nil with no config, got %v", result)
	}

	// Project config with grammar key (comma-separated)
	projectCfg := ConfigMap{
		"ast": map[string]any{
			"grammar": ".sql=antlr-plsql,.ts=tree-sitter-typescript",
		},
	}
	result = ResolveGrammarOverrides(nil, projectCfg)
	if len(result) != 2 {
		t.Fatalf("expected 2 entries, got %d: %v", len(result), result)
	}
	if result[".sql"] != "antlr-plsql" {
		t.Errorf(".sql: expected antlr-plsql, got %q", result[".sql"])
	}
	if result[".ts"] != "tree-sitter-typescript" {
		t.Errorf(".ts: expected tree-sitter-typescript, got %q", result[".ts"])
	}

	// Inline config takes precedence over project config
	inlineCfg := ConfigMap{
		"ast": map[string]any{
			"grammar": ".sql=tree-sitter-sql",
		},
	}
	result = ResolveGrammarOverrides(inlineCfg, projectCfg)
	if result[".sql"] != "tree-sitter-sql" {
		t.Errorf(".sql: expected tree-sitter-sql from inline, got %q", result[".sql"])
	}
	// inline replaces the entire value, so .ts is not present
	if _, ok := result[".ts"]; ok {
		t.Error(".ts should not be present when inline overrides")
	}
}
