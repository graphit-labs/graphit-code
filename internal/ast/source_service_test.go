package ast

import (
	"testing"
)

// ---------------------------------------------------------------------------
// formatMatches
// ---------------------------------------------------------------------------

func TestFormatMatches_Basic(t *testing.T) {
	matches := []SourceMatch{
		{LineNumber: 1, Line: "package main", IsMatch: true},
		{LineNumber: 2, Line: "", IsMatch: false},
		{LineNumber: 3, Line: "func main() {", IsMatch: true},
	}
	got := formatMatches(matches)

	// All consecutive, no separator expected
	if got == "" {
		t.Fatal("expected non-empty output")
	}
	// Check match markers
	if !contains(got, ">    1: package main") {
		t.Errorf("expected match marker for line 1, got:\n%s", got)
	}
	if !contains(got, "     2:") {
		t.Errorf("expected context marker for line 2, got:\n%s", got)
	}
	if !contains(got, ">    3: func main() {") {
		t.Errorf("expected match marker for line 3, got:\n%s", got)
	}
}

func TestFormatMatches_SeparatorOnGap(t *testing.T) {
	matches := []SourceMatch{
		{LineNumber: 1, Line: "first", IsMatch: true},
		{LineNumber: 5, Line: "fifth", IsMatch: true}, // gap between 1 and 5
	}
	got := formatMatches(matches)
	if !contains(got, "---") {
		t.Errorf("expected separator for gap, got:\n%s", got)
	}
}

func TestFormatMatches_NoSeparatorConsecutive(t *testing.T) {
	matches := []SourceMatch{
		{LineNumber: 10, Line: "a", IsMatch: true},
		{LineNumber: 11, Line: "b", IsMatch: false},
	}
	got := formatMatches(matches)
	if contains(got, "---") {
		t.Errorf("should not have separator for consecutive lines, got:\n%s", got)
	}
}

func TestFormatMatches_Empty(t *testing.T) {
	got := formatMatches(nil)
	if got != "" {
		t.Errorf("expected empty string for nil matches, got %q", got)
	}
}

// ---------------------------------------------------------------------------
// formatWithLineNumbers
// ---------------------------------------------------------------------------

func TestFormatWithLineNumbers(t *testing.T) {
	tests := []struct {
		name   string
		lines  []string
		offset int
		want   string
	}{
		{
			name:   "simple",
			lines:  []string{"alpha", "beta"},
			offset: 1,
			want:   "   1: alpha\n   2: beta",
		},
		{
			name:   "offset_100",
			lines:  []string{"foo"},
			offset: 100,
			want:   " 100: foo",
		},
		{
			name:   "empty_lines",
			lines:  []string{},
			offset: 1,
			want:   "",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := formatWithLineNumbers(tt.lines, tt.offset)
			if got != tt.want {
				t.Errorf("got %q, want %q", got, tt.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// sortInts
// ---------------------------------------------------------------------------

func TestSortInts(t *testing.T) {
	tests := []struct {
		name  string
		input []int
		want  []int
	}{
		{"already_sorted", []int{1, 2, 3}, []int{1, 2, 3}},
		{"reverse", []int{5, 3, 1}, []int{1, 3, 5}},
		{"duplicates", []int{3, 1, 3, 2}, []int{1, 2, 3, 3}},
		{"single", []int{42}, []int{42}},
		{"empty", []int{}, []int{}},
		{"nil", nil, nil},
		{"negative", []int{-1, 5, -10, 0}, []int{-10, -1, 0, 5}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			sortInts(tt.input)
			if len(tt.input) != len(tt.want) {
				t.Fatalf("length mismatch: got %v, want %v", tt.input, tt.want)
			}
			for i := range tt.input {
				if tt.input[i] != tt.want[i] {
					t.Errorf("index %d: got %d, want %d (full: %v)", i, tt.input[i], tt.want[i], tt.input)
					break
				}
			}
		})
	}
}

// ---------------------------------------------------------------------------
// SourceRequest / SourceResult types sanity
// ---------------------------------------------------------------------------

func TestSourceRequestDefaults(t *testing.T) {
	req := SourceRequest{}
	if req.Path != "" {
		t.Error("expected empty path")
	}
	if req.Head != 0 || req.Tail != 0 {
		t.Error("expected zero head/tail")
	}
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

func contains(s, substr string) bool {
	return len(s) >= len(substr) && searchSubstring(s, substr)
}

func searchSubstring(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
