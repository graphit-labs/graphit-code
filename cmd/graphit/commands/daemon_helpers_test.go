package commands

import (
	"testing"
)

func TestSplitLines(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		want  int
	}{
		{"empty", "", 0},
		{"single_line_no_newline", "hello", 1},
		{"single_line_with_newline", "hello\n", 1},
		{"two_lines", "hello\nworld", 2},
		{"two_lines_trailing", "hello\nworld\n", 2},
		{"multiple_empty_lines", "\n\n\n", 3},
		{"mixed", "line1\nline2\nline3", 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := splitLines(tt.input)
			if len(got) != tt.want {
				t.Errorf("splitLines(%q) = %d lines; want %d (lines=%v)", tt.input, len(got), tt.want, got)
			}
		})
	}
}

func TestSplitLastN(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		n     int
		want  int
	}{
		{"empty", "", 5, 0},
		{"fewer_than_n", "line1\nline2\n", 5, 2},
		{"exact_n", "line1\nline2\nline3\n", 3, 3},
		{"more_than_n", "line1\nline2\nline3\nline4\nline5\n", 3, 3},
		{"n_is_1", "a\nb\nc\n", 1, 1},
		{"skips_empty_lines", "line1\n\nline2\n\nline3\n", 10, 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := splitLastN(tt.input, tt.n)
			if len(got) != tt.want {
				t.Errorf("splitLastN(%q, %d) = %d lines; want %d (lines=%v)", tt.input, tt.n, len(got), tt.want, got)
			}
		})
	}

	t.Run("returns_last_entries", func(t *testing.T) {
		t.Parallel()
		got := splitLastN("a\nb\nc\nd\ne\n", 2)
		if len(got) != 2 {
			t.Fatalf("expected 2 lines, got %d", len(got))
		}
		if got[0] != "d" || got[1] != "e" {
			t.Errorf("expected [d e], got %v", got)
		}
	})
}
