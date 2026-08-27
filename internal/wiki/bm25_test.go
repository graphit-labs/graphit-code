package wiki

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDefaultBM25Config(t *testing.T) {
	t.Parallel()
	cfg := DefaultBM25Config()
	if cfg.K1 != 1.2 {
		t.Errorf("K1 = %v, want 1.2", cfg.K1)
	}
	if cfg.B != 0.75 {
		t.Errorf("B = %v, want 0.75", cfg.B)
	}
}

func TestNewBM25Index(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "alpha.md", "# Alpha\nSome alpha content here.")
	writeFile(t, dir, "beta.md", "# Beta\nBeta content is different.")
	writeFile(t, dir, "not-md.txt", "should be ignored")

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if idx.totalDocs != 2 {
		t.Errorf("totalDocs = %d, want 2", idx.totalDocs)
	}
	if idx.avgDocLen == 0 {
		t.Error("avgDocLen should not be zero with indexed docs")
	}
}

func TestNewBM25Index_EmptyDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if idx.totalDocs != 0 {
		t.Errorf("totalDocs = %d, want 0", idx.totalDocs)
	}
}

func TestNewBM25Index_InvalidDir(t *testing.T) {
	t.Parallel()
	_, err := NewBM25Index(filepath.Join(t.TempDir(), "nonexistent"), DefaultBM25Config())
	if err != nil {
		t.Fatalf("WalkDir on nonexistent should not return error (it skips), got: %v", err)
	}
}

func TestBM25Search(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "golang.md", "# Golang\nGolang is a programming language developed by Google.")
	writeFile(t, dir, "python.md", "# Python\nPython is a scripting language popular for data science.")
	writeFile(t, dir, "rust.md", "# Rust\nRust is a systems programming language focused on safety.")

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	results := idx.Search("programming language", 10)
	if len(results) == 0 {
		t.Fatal("expected search results, got none")
	}

	for _, r := range results {
		if r.Score <= 0 {
			t.Errorf("result %q has non-positive score: %v", r.Path, r.Score)
		}
	}
}

func TestBM25Search_TopN(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	for i := 0; i < 10; i++ {
		writeFile(t, dir, filepath.Base(t.TempDir())+".md", "keyword repeated keyword keyword")
	}
	writeFile(t, dir, "a.md", "keyword here")
	writeFile(t, dir, "b.md", "keyword here too")
	writeFile(t, dir, "c.md", "keyword also present")

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	results := idx.Search("keyword", 2)
	if len(results) > 2 {
		t.Errorf("expected at most 2 results with topN=2, got %d", len(results))
	}
}

func TestBM25SearchEmptyQuery(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "doc.md", "# Doc\nSome content here.")

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	results := idx.Search("", 10)
	if results != nil {
		t.Errorf("expected nil for empty query, got %v", results)
	}
}

func TestBM25SearchSpellingCorrection(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "architecture.md", "# Architecture\nThe architecture of the system involves microservices.")

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// "architectur" is a misspelling that should trigger trigram correction
	results := idx.Search("architectur", 5)
	if len(results) == 0 {
		t.Error("expected spelling correction to find results for misspelled query")
	}
}

func TestBM25Search_NoMatchingTerms(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "doc.md", "# Doc\nalpha beta gamma")

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	results := idx.Search("zzzzxyzzy", 5)
	if len(results) != 0 {
		t.Errorf("expected no results for completely unrelated query, got %d", len(results))
	}
}

func TestTokenize(t *testing.T) {
	t.Parallel()

	idx := &BM25Index{stopwords: defaultStopwords()}

	tests := []struct {
		name  string
		input string
		want  []string
	}{
		{"empty", "", nil},
		{"stopwords_removed", "the and or", nil},
		{"short_tokens_removed", "a b c", nil},
		{"separators_split", "hello-world_test", []string{"hello-world_test"}},
		{"mixed_content", "Golang programming 123", []string{"golang", "programming", "123"}},
		{"punctuation_split", "hello, world! test.", []string{"hello", "world", "test"}},
		{"underscores_trimmed", "_hello_", []string{"hello"}},
		{"dashes_trimmed", "-hello-", []string{"hello"}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := idx.tokenize(tt.input)
			if len(got) != len(tt.want) {
				t.Errorf("tokenize(%q) = %v (len %d), want %v (len %d)",
					tt.input, got, len(got), tt.want, len(tt.want))
				return
			}
			for i := range got {
				if got[i] != tt.want[i] {
					t.Errorf("tokenize(%q)[%d] = %q, want %q", tt.input, i, got[i], tt.want[i])
				}
			}
		})
	}
}

func TestExtractBM25Title(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"with_h1", "# My Title\nBody text.", "My Title"},
		{"no_h1", "No heading here.\nJust text.", ""},
		{"h1_with_spaces", "#   Spaced Title  \nBody.", "Spaced Title"},
		{"h2_not_matched", "## Not H1\nBody.", ""},
		{"frontmatter_then_h1", "---\ntitle: test\n---\n# Actual Title\nBody.", "Actual Title"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := extractBM25Title(tt.content)
			if got != tt.want {
				t.Errorf("extractBM25Title() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestStripYAMLFrontmatter(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			"with_frontmatter",
			"---\ntitle: test\ntags: [a]\n---\n# Body\nContent.",
			"# Body\nContent.",
		},
		{
			"no_frontmatter",
			"# Just Content\nNo frontmatter.",
			"# Just Content\nNo frontmatter.",
		},
		{
			"unclosed_frontmatter",
			"---\ntitle: test\nno closing delimiter\n# Body",
			"",
		},
		{
			"empty_frontmatter",
			"---\n---\n# Body",
			"# Body",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := StripFrontmatter(tt.content)
			if got != tt.want {
				t.Errorf("StripFrontmatter() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestBM25SearchFunc(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "memory.md", "# Memory Management\nMemory management involves allocation and deallocation.")
	writeFile(t, dir, "cpu.md", "# CPU Architecture\nCPU design and instruction sets.")

	results := BM25Search(context.Background(), dir, "memory management", 5)
	if len(results) == 0 {
		t.Fatal("expected BM25Search to return results")
	}
	if results[0].Path != "memory.md" {
		t.Errorf("top result path = %q, want memory.md", results[0].Path)
	}
}

func TestBM25SearchFunc_InvalidDir(t *testing.T) {
	t.Parallel()
	results := BM25Search(context.Background(), filepath.Join(t.TempDir(), "nonexistent"), "query", 5)
	if results != nil {
		t.Errorf("expected nil for invalid dir, got %v", results)
	}
}

func TestBM25SearchFunc_WithSnippets(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "doc.md", "# Doc\nThis document discusses memory allocation strategies for modern systems.")

	results := BM25Search(context.Background(), dir, "memory allocation", 5)
	if len(results) == 0 {
		t.Fatal("expected results")
	}
	if results[0].Snippet == "" {
		t.Error("expected snippet to be populated")
	}
}

func TestBm25PreFilter(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "alpha.md", "# Alpha\nAlpha content with specific keywords.")
	writeFile(t, dir, "beta.md", "# Beta\nBeta content unrelated.")

	result := bm25PreFilter(context.Background(), dir, "alpha keywords", 5)
	if result == "" {
		t.Fatal("expected non-empty pre-filter result")
	}
	if !strings.Contains(result, "BM25 Relevant Pages") {
		t.Error("expected header in pre-filter output")
	}
	if !strings.Contains(result, "alpha") {
		t.Error("expected alpha in results")
	}
}

func TestBm25PreFilter_EmptyDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	result := bm25PreFilter(context.Background(), dir, "query", 5)
	if result != "" {
		t.Errorf("expected empty result for empty dir, got %q", result)
	}
}

func TestBm25PreFilter_NoMatch(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "doc.md", "# Doc\nalpha beta gamma")
	result := bm25PreFilter(context.Background(), dir, "zzzzxyzzy", 5)
	if result != "" {
		t.Errorf("expected empty result for no-match query, got %q", result)
	}
}

func TestBm25PreFilter_WithTitle(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "doc.md", "# My Great Title\nSome specific searchterm content.")

	result := bm25PreFilter(context.Background(), dir, "searchterm", 5)
	if !strings.Contains(result, "My Great Title") {
		t.Error("expected title in pre-filter output")
	}
}

func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatalf("writeFile(%q): %v", name, err)
	}
}
