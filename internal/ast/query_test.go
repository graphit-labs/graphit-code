package ast

import (
	"testing"
)

// ---------------------------------------------------------------------------
// levenshtein
// ---------------------------------------------------------------------------

func TestLevenshtein(t *testing.T) {
	tests := []struct {
		a    string
		b    string
		want int
	}{
		{"", "", 0},
		{"abc", "", 3},
		{"", "xyz", 3},
		{"abc", "abc", 0},
		{"kitten", "sitting", 3},
		{"saturday", "sunday", 3},
		{"abc", "axc", 1},
		{"abc", "ab", 1},
		{"a", "b", 1},
		{"abc", "def", 3},
	}
	for _, tt := range tests {
		t.Run(tt.a+"_"+tt.b, func(t *testing.T) {
			got := levenshtein(tt.a, tt.b)
			if got != tt.want {
				t.Errorf("levenshtein(%q, %q) = %d, want %d", tt.a, tt.b, got, tt.want)
			}
			// Should be symmetric
			got2 := levenshtein(tt.b, tt.a)
			if got2 != tt.want {
				t.Errorf("levenshtein(%q, %q) = %d, want %d (symmetry)", tt.b, tt.a, got2, tt.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// min3
// ---------------------------------------------------------------------------

func TestMin3(t *testing.T) {
	tests := []struct {
		a, b, c int
		want    int
	}{
		{1, 2, 3, 1},
		{3, 2, 1, 1},
		{2, 1, 3, 1},
		{5, 5, 5, 5},
		{0, 0, 0, 0},
		{-1, 0, 1, -1},
		{100, 50, 75, 50},
	}
	for _, tt := range tests {
		got := min3(tt.a, tt.b, tt.c)
		if got != tt.want {
			t.Errorf("min3(%d, %d, %d) = %d, want %d", tt.a, tt.b, tt.c, got, tt.want)
		}
	}
}

// ---------------------------------------------------------------------------
// recordToResult
// ---------------------------------------------------------------------------

func TestRecordToResult(t *testing.T) {
	t.Run("full_record", func(t *testing.T) {
		rec := QueryRecord{
			"name":          "MyFunc",
			"path":          "pkg/service.go",
			"line_number":   42,
			"source":        "func MyFunc() {}",
			"docstring":     "does stuff",
			"is_dependency": false,
		}
		sr := recordToResult(rec, "function")
		if sr.Type != "function" {
			t.Errorf("type: got %q", sr.Type)
		}
		if sr.Name != "MyFunc" {
			t.Errorf("name: got %q", sr.Name)
		}
		if sr.Path != "pkg/service.go" {
			t.Errorf("path: got %q", sr.Path)
		}
		if sr.Line != 42 {
			t.Errorf("line: got %d", sr.Line)
		}
		if sr.Source != "func MyFunc() {}" {
			t.Errorf("source: got %q", sr.Source)
		}
		if sr.Docstring != "does stuff" {
			t.Errorf("docstring: got %q", sr.Docstring)
		}
		if sr.IsDepend {
			t.Error("is_dependency should be false")
		}
	})

	t.Run("line_number_float64", func(t *testing.T) {
		rec := QueryRecord{"line_number": float64(99)}
		sr := recordToResult(rec, "class")
		if sr.Line != 99 {
			t.Errorf("expected line 99, got %d", sr.Line)
		}
	})

	t.Run("line_number_int64", func(t *testing.T) {
		rec := QueryRecord{"line_number": int64(77)}
		sr := recordToResult(rec, "class")
		if sr.Line != 77 {
			t.Errorf("expected line 77, got %d", sr.Line)
		}
	})

	t.Run("missing_fields", func(t *testing.T) {
		rec := QueryRecord{}
		sr := recordToResult(rec, "variable")
		if sr.Type != "variable" {
			t.Errorf("type: got %q", sr.Type)
		}
		if sr.Name != "" || sr.Path != "" || sr.Line != 0 {
			t.Error("missing fields should result in zero values")
		}
	})
}

// ---------------------------------------------------------------------------
// recordsToResults
// ---------------------------------------------------------------------------

func TestRecordsToResults(t *testing.T) {
	records := []QueryRecord{
		{"name": "A", "path": "a.go"},
		{"name": "B", "path": "b.go"},
	}
	results := recordsToResults(records, "function")
	if len(results) != 2 {
		t.Fatalf("expected 2 results, got %d", len(results))
	}
	if results[0].Name != "A" || results[1].Name != "B" {
		t.Errorf("unexpected results: %v", results)
	}
}

// ---------------------------------------------------------------------------
// recordsToMaps
// ---------------------------------------------------------------------------

func TestRecordsToMaps(t *testing.T) {
	records := []QueryRecord{
		{"k1": "v1", "k2": 42},
	}
	maps := recordsToMaps(records)
	if len(maps) != 1 {
		t.Fatalf("expected 1 map, got %d", len(maps))
	}
	if maps[0]["k1"] != "v1" {
		t.Errorf("k1: got %v", maps[0]["k1"])
	}
	if maps[0]["k2"] != 42 {
		t.Errorf("k2: got %v", maps[0]["k2"])
	}

	// Should not share reference with original
	maps[0]["k1"] = "modified"
	if records[0]["k1"] != "v1" {
		t.Error("recordsToMaps should copy, not share reference")
	}
}

// ---------------------------------------------------------------------------
// UID (from types.go)
// ---------------------------------------------------------------------------

func TestUID(t *testing.T) {
	tests := []struct {
		parts []string
		want  string
	}{
		{[]string{"a", "b", "c"}, "abc"},
		{[]string{"hello"}, "hello"},
		{[]string{}, ""},
		{[]string{"", ""}, ""},
		{[]string{"path/to/file", "::", "FuncName"}, "path/to/file::FuncName"},
	}
	for _, tt := range tests {
		got := UID(tt.parts...)
		if got != tt.want {
			t.Errorf("UID(%v) = %q, want %q", tt.parts, got, tt.want)
		}
	}
}

// ---------------------------------------------------------------------------
// buildNodeNameQuery
// ---------------------------------------------------------------------------

func TestBuildNodeNameQuery(t *testing.T) {
	t.Run("exact_no_repo", func(t *testing.T) {
		q := buildNodeNameQuery("Function", false, "")
		if q == "" {
			t.Fatal("expected non-empty query")
		}
		if !stringContains(q, "Function") {
			t.Error("should contain label")
		}
		if !stringContains(q, "node.name = $name") {
			t.Error("should use exact match for non-fuzzy")
		}
		if stringContains(q, "repo_path") {
			t.Error("should not have repo filter")
		}
	})

	t.Run("fuzzy_with_repo", func(t *testing.T) {
		q := buildNodeNameQuery("Class", true, "/project")
		if !stringContains(q, "CONTAINS") {
			t.Error("should use CONTAINS for fuzzy")
		}
		if !stringContains(q, "repo_path") {
			t.Error("should have repo filter")
		}
	})
}

func stringContains(s, sub string) bool {
	return len(s) >= len(sub) && searchSubstr(s, sub)
}

func searchSubstr(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
