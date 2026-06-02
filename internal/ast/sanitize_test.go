package ast

import (
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// SanitizeProps
// ---------------------------------------------------------------------------

func TestSanitizeProps(t *testing.T) {
	t.Run("preserves_basic_types", func(t *testing.T) {
		input := map[string]any{
			"str":   "hello",
			"int":   42,
			"int64": int64(99),
			"float": 3.14,
			"bool":  true,
			"nil":   nil,
		}
		out := SanitizeProps(input)
		if out["str"] != "hello" {
			t.Errorf("str: got %v", out["str"])
		}
		if out["int"] != 42 {
			t.Errorf("int: got %v", out["int"])
		}
		if out["int64"] != int64(99) {
			t.Errorf("int64: got %v", out["int64"])
		}
		if out["float"] != 3.14 {
			t.Errorf("float: got %v", out["float"])
		}
		if out["bool"] != true {
			t.Errorf("bool: got %v", out["bool"])
		}
		if out["nil"] != nil {
			t.Errorf("nil: got %v", out["nil"])
		}
	})

	t.Run("truncates_long_strings", func(t *testing.T) {
		long := strings.Repeat("x", MaxStrLen+100)
		out := SanitizeProps(map[string]any{"long": long})
		s, ok := out["long"].(string)
		if !ok {
			t.Fatalf("expected string, got %T", out["long"])
		}
		if len(s) != MaxStrLen {
			t.Errorf("expected truncation to %d, got %d", MaxStrLen, len(s))
		}
	})

	t.Run("truncates_string_array_elements", func(t *testing.T) {
		long := strings.Repeat("a", MaxStrLen+50)
		out := SanitizeProps(map[string]any{"arr": []string{long, "short"}})
		arr, ok := out["arr"].([]string)
		if !ok {
			t.Fatalf("expected []string, got %T", out["arr"])
		}
		if len(arr) != 2 {
			t.Fatalf("expected 2 elements, got %d", len(arr))
		}
		if len(arr[0]) != MaxStrLen {
			t.Errorf("expected first element truncated to %d, got %d", MaxStrLen, len(arr[0]))
		}
		if arr[1] != "short" {
			t.Errorf("expected second element 'short', got %q", arr[1])
		}
	})

	t.Run("preserves_flat_any_list", func(t *testing.T) {
		input := map[string]any{
			"flat": []any{"a", 1, true, nil},
		}
		out := SanitizeProps(input)
		flat, ok := out["flat"].([]any)
		if !ok {
			t.Fatalf("expected []any, got %T", out["flat"])
		}
		if len(flat) != 4 {
			t.Errorf("expected 4 elements, got %d", len(flat))
		}
	})

	t.Run("complex_values_serialized_to_json", func(t *testing.T) {
		nested := map[string]any{
			"complex": []any{map[string]any{"key": "value"}},
		}
		out := SanitizeProps(nested)
		s, ok := out["complex"].(string)
		if !ok {
			t.Fatalf("expected string (JSON), got %T", out["complex"])
		}
		if s == "" {
			t.Error("expected non-empty JSON string")
		}
	})

	t.Run("does_not_mutate_original", func(t *testing.T) {
		original := map[string]any{"key": "value"}
		out := SanitizeProps(original)
		out["key"] = "modified"
		if original["key"] != "value" {
			t.Error("SanitizeProps should not mutate original map")
		}
	})

	t.Run("empty_map", func(t *testing.T) {
		out := SanitizeProps(map[string]any{})
		if len(out) != 0 {
			t.Errorf("expected empty map, got %v", out)
		}
	})
}

// ---------------------------------------------------------------------------
// isFlatList
// ---------------------------------------------------------------------------

func TestIsFlatList(t *testing.T) {
	tests := []struct {
		name  string
		input []any
		want  bool
	}{
		{"all_strings", []any{"a", "b"}, true},
		{"all_ints", []any{1, 2, 3}, true},
		{"mixed_primitives", []any{"a", 1, 3.14, true, nil}, true},
		{"nested_map", []any{map[string]any{"k": "v"}}, false},
		{"nested_slice", []any{[]any{1}}, false},
		{"empty", []any{}, true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := isFlatList(tt.input)
			if got != tt.want {
				t.Errorf("isFlatList(%v) = %v, want %v", tt.input, got, tt.want)
			}
		})
	}
}
