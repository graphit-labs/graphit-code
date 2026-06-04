package wiki

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// ─── Mock implementations ────────────────────────────────────────────────────

type mockAIClient struct {
	responses []string
	calls     int
}

func (m *mockAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	m.calls++
	if m.calls <= len(m.responses) {
		return m.responses[m.calls-1], nil
	}
	return "DONE: default response", nil
}

type errAIClient struct{}

func (e *errAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	return "", fmt.Errorf("ai error")
}

type mockGraphAdapter struct {
	queryResults  map[string]*QueryResult
	execResults   map[string]*QueryResult
	queryErr      error
	execErr       error
	queryCalls    int
	executeCalls  int
	closeCalled   bool
	queryCypher   []string
	executeCypher []string
}

func (m *mockGraphAdapter) Query(_ context.Context, cypher string, _ map[string]any) (*QueryResult, error) {
	m.queryCalls++
	m.queryCypher = append(m.queryCypher, cypher)
	if m.queryErr != nil {
		return nil, m.queryErr
	}
	if r, ok := m.queryResults[cypher]; ok {
		return r, nil
	}
	return &QueryResult{}, nil
}

func (m *mockGraphAdapter) Execute(_ context.Context, cypher string, _ map[string]any) (*QueryResult, error) {
	m.executeCalls++
	m.executeCypher = append(m.executeCypher, cypher)
	if m.execErr != nil {
		return nil, m.execErr
	}
	if r, ok := m.execResults[cypher]; ok {
		return r, nil
	}
	return &QueryResult{}, nil
}

func (m *mockGraphAdapter) Close() error {
	m.closeCalled = true
	return nil
}

type mockExtractor struct {
	results map[string]*ExtractionResult
}

func (m *mockExtractor) Extract(relPath, _ string) *ExtractionResult {
	if r, ok := m.results[relPath]; ok {
		return r
	}
	return nil
}

type mockGraphWriter struct {
	writeErr    error
	deleteErr   error
	writeCalls  int
	deleteCalls int
	stats       WriteStats
}

func (m *mockGraphWriter) WriteResult(_ context.Context, r *ExtractionResult) error {
	m.writeCalls++
	if m.writeErr != nil {
		return m.writeErr
	}
	m.stats.NodesWritten += len(r.Nodes)
	m.stats.EdgesWritten += len(r.Edges)
	return nil
}

func (m *mockGraphWriter) DeleteDocument(_ context.Context, _ string) error {
	m.deleteCalls++
	return m.deleteErr
}

func (m *mockGraphWriter) Stats() WriteStats {
	return m.stats
}

type mockRenderer struct {
	entityPageContent string
	indexPageContent  string
	logTitle          string
	moduleTag         string
}

func (m *mockRenderer) EntityPage(_ context.Context, _ GraphAdapter, _ EntitySummary) string {
	return m.entityPageContent
}

func (m *mockRenderer) IndexPage(_ []EntitySummary, _ []Community, _ []map[string]any, _, _ int, _ string) string {
	return m.indexPageContent
}

func (m *mockRenderer) LogTitle() string {
	return m.logTitle
}

func (m *mockRenderer) ModuleTag() string {
	return m.moduleTag
}

// ─── Helper functions ────────────────────────────────────────────────────────

func createWikiDir(t *testing.T, pages map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	for name, content := range pages {
		fullPath := filepath.Join(dir, name)
		if parent := filepath.Dir(fullPath); parent != dir {
			if err := os.MkdirAll(parent, 0o755); err != nil {
				t.Fatalf("failed to create dir for %s: %v", name, err)
			}
		}
		if err := os.WriteFile(fullPath, []byte(content), 0o644); err != nil {
			t.Fatalf("failed to write %s: %v", name, err)
		}
	}
	return dir
}

// ─── validate.go tests ──────────────────────────────────────────────────────

func TestIsPageRefLine(t *testing.T) {
	tests := []struct {
		line string
		want bool
	}{
		{"[[Some_Page]]", true},
		{"[source-1]/[[Some_Page]]", true},
		{"src/doc.md", true},
		{"my_doc_page_ref", true},
		{"my doc page ref", false},
		{"word", false},
		{"simple text with spaces", false},
		// Edge: underscores < 2
		{"a_b", false},
		// Edge: short underscore-only
		{"a_b_c", false}, // <= 10 chars
	}
	for _, tc := range tests {
		t.Run(tc.line, func(t *testing.T) {
			got := isPageRefLine(tc.line)
			if got != tc.want {
				t.Errorf("isPageRefLine(%q) = %t; want %t", tc.line, got, tc.want)
			}
		})
	}
}

func TestIsPageRefOnlyAnswer(t *testing.T) {
	tests := []struct {
		name   string
		answer string
		want   bool
	}{
		{"empty", "", true},
		{"whitespace", "  \n  ", true},
		{"wikilinks_list", "- [[Page_One]]\n- [[Page_Two]]", true},
		{"numbered_list", "1. [[Page_One]]\n2. [[Page_Two]]", true},
		{"normal_paragraph", "This is a normal paragraph answer explaining details.", false},
		{"mixed_content", "- [[Page_One]]\nThis is normal explanation line.", false},
		// Cover numbered list prefix removal for digits 3-9
		{"numbered_3to9", "3. [[Page_Three]]\n4. [[Page_Four]]\n5. [[Page_Five]]\n6. [[Page_Six]]\n7. [[Page_Seven]]\n8. [[Page_Eight]]\n9. [[Page_Nine]]", true},
		// Cover asterisk prefixed list
		{"asterisk_list", "* [[Page_A]]\n* [[Page_B]]", true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := isPageRefOnlyAnswer(tc.answer)
			if got != tc.want {
				t.Errorf("isPageRefOnlyAnswer(%q) = %t; want %t", tc.answer, got, tc.want)
			}
		})
	}
}

func TestBuildSynthesisRetryPrompt(t *testing.T) {
	prompt := buildSynthesisRetryPrompt("my query", "my context")
	if !strings.Contains(prompt, "my query") || !strings.Contains(prompt, "my context") {
		t.Errorf("unexpected prompt: %s", prompt)
	}
}

// ─── crossref.go tests ──────────────────────────────────────────────────────

func TestBuildCrossRefGraph(t *testing.T) {
	pages := map[string]string{
		"index.md":    "# Index\n- [[Page_A]]\n- [[Page_B]]",
		"Page_A.md":   "# Page A\nSee [[Page_B]] and [[Page_C]]",
		"Page_B.md":   "# Page B\nSee [[Page_A]]",
		"orphan.md":   "# Orphan\nNo links at all",
		"notmd.txt":   "not markdown",
		"subdir/x.md": "nested file",
	}
	dir := createWikiDir(t, pages)
	// Create a subdirectory (should be skipped)
	_ = os.MkdirAll(filepath.Join(dir, "subdir"), 0o755)

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("BuildCrossRefGraph error: %v", err)
	}

	if !graph.AllPages["Page_A"] || !graph.AllPages["Page_B"] {
		t.Error("expected pages to be registered")
	}

	// Check outbound links
	if len(graph.Outbound["Page_A"]) != 2 {
		t.Errorf("expected 2 outbound from Page_A, got %d", len(graph.Outbound["Page_A"]))
	}
	// Check inbound (from index and Page_B)
	if len(graph.Inbound["Page_A"]) != 2 {
		t.Errorf("expected 2 inbound to Page_A, got %d", len(graph.Inbound["Page_A"]))
	}
	// Check titles
	if graph.Titles["Page_A"] != "Page A" {
		t.Errorf("expected title 'Page A', got %q", graph.Titles["Page_A"])
	}
}

func TestBuildCrossRefGraphError(t *testing.T) {
	_, err := BuildCrossRefGraph("/nonexistent/path")
	if err == nil {
		t.Error("expected error for nonexistent path")
	}
}

func TestInjectBacklinks(t *testing.T) {
	pages := map[string]string{
		"index.md":  "# Index\n",
		"log.md":    "# Log\n",
		"Page_A.md": "# Page A\nSee [[Page_B]]\n---\n*Navigate: [[index]]*",
		"Page_B.md": "# Page B\nSee [[Page_A]]\n---\n*Navigate: [[index]]*",
	}
	dir := createWikiDir(t, pages)

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("BuildCrossRefGraph error: %v", err)
	}

	result, err := InjectBacklinks(dir, graph)
	if err != nil {
		t.Fatalf("InjectBacklinks error: %v", err)
	}

	if result.TotalPages < 2 {
		t.Errorf("expected at least 2 total pages, got %d", result.TotalPages)
	}

	// Verify backlinks were injected
	data, _ := os.ReadFile(filepath.Join(dir, "Page_A.md"))
	if !strings.Contains(string(data), "## Backlinks") {
		t.Error("expected backlinks section in Page_A")
	}

	// Run again - should not change (already injected)
	result2, _ := InjectBacklinks(dir, graph)
	if result2.BacklinksAdded != 0 {
		t.Errorf("expected 0 backlinks on re-inject, got %d", result2.BacklinksAdded)
	}
}

func TestOrphanPages(t *testing.T) {
	graph := &CrossRefGraph{
		AllPages: map[string]bool{
			"index":  true,
			"log":    true,
			"Page_A": true,
			"orphan": true,
		},
		Outbound: map[string][]string{
			"Page_A": {"some_target"},
		},
		Inbound: map[string][]string{},
	}

	orphans := OrphanPages(graph)
	if len(orphans) != 1 || orphans[0] != "orphan" {
		t.Errorf("expected orphan page, got %v", orphans)
	}
}

func TestBrokenLinks(t *testing.T) {
	graph := &CrossRefGraph{
		AllPages: map[string]bool{
			"Page_A": true,
		},
		Outbound: map[string][]string{
			"Page_A": {"NonExistent", "AlsoMissing"},
		},
	}

	broken := BrokenLinks(graph)
	if len(broken) != 2 {
		t.Errorf("expected 2 broken links, got %d", len(broken))
	}
}

func TestStripBacklinksSection(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"no_backlinks", "# Hello\nWorld", "# Hello\nWorld"},
		{"with_backlinks", "# Hello\nContent\n## Backlinks\n- link1", "# Hello\nContent"},
		{"starts_with_backlinks", "## Backlinks\n- link1", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := stripBacklinksSection(tc.content)
			if got != tc.want {
				t.Errorf("stripBacklinksSection(%q) = %q; want %q", tc.content, got, tc.want)
			}
		})
	}
}

func TestInjectBacklinksSection(t *testing.T) {
	content := "# Page\nContent"
	inbound := []string{"src1", "src2"}
	titles := map[string]string{
		"src1": "Source One",
		"src2": "src2", // same as slug = no title suffix
	}

	result := injectBacklinksSection(content, inbound, titles)
	if !strings.Contains(result, "## Backlinks") {
		t.Error("expected backlinks header")
	}
	if !strings.Contains(result, "[[src1]] — Source One") {
		t.Error("expected titled backlink")
	}
	if !strings.Contains(result, "- [[src2]]\n") {
		t.Error("expected untitled backlink")
	}
}

func TestStripCodeBlocks(t *testing.T) {
	content := "Hello\n```go\ncode here\n```\nWorld\nInline `code` here"
	result := stripCodeBlocks(content)
	if strings.Contains(result, "code here") {
		t.Error("expected fenced code to be stripped")
	}
	if strings.Contains(result, "code") && strings.Contains(result, "`") {
		t.Error("expected inline code to be stripped")
	}
	if !strings.Contains(result, "Hello") || !strings.Contains(result, "World") {
		t.Error("expected non-code text to remain")
	}
}

func TestResolveSlug(t *testing.T) {
	tests := []struct {
		rawLink string
		want    string
	}{
		{"Some_Page", "Some_Page"},
		{"Some Page", "Some_Page"},
		{"Some Page|Display Label", "Some_Page"},
		{"  Some Page  |  Display Label  ", "Some_Page"},
		{"", ""},
	}
	for _, tc := range tests {
		t.Run(tc.rawLink, func(t *testing.T) {
			got := ResolveSlug(tc.rawLink)
			if got != tc.want {
				t.Errorf("ResolveSlug(%q) = %q; want %q", tc.rawLink, got, tc.want)
			}
		})
	}
}

func TestFindWikiLinks(t *testing.T) {
	content := "This is a link to [[Some Page|Display Label]] and another link to [[Other_Page]].\n" +
		"```go\n// [[Ignored Page]]\n```\n" +
		"Also this `[[Ignored Inline]]` should be ignored."
	got := FindWikiLinks(content)
	want := []string{"Some_Page", "Other_Page"}
	if len(got) != len(want) {
		t.Fatalf("FindWikiLinks length = %d; want %d (got: %v)", len(got), len(want), got)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("FindWikiLinks[%d] = %q; want %q", i, got[i], want[i])
		}
	}
}

// ─── bm25.go tests ──────────────────────────────────────────────────────────

func TestBM25Index(t *testing.T) {
	dir := t.TempDir()

	doc1 := "---\ntitle: Doc 1\ntags: [tag]\n---\n# Document One Title\nThis is the body content of document one. It describes the design patterns and Go guidelines."
	doc2 := "\n# Document Two Title\nThis is another document detailing the implementation and test coverage. It focuses on React UI."
	_ = os.WriteFile(filepath.Join(dir, "doc1.md"), []byte(doc1), 0o644)
	_ = os.WriteFile(filepath.Join(dir, "doc2.md"), []byte(doc2), 0o644)
	// Non-md file should be ignored
	_ = os.WriteFile(filepath.Join(dir, "readme.txt"), []byte("ignored"), 0o644)

	idx, err := NewBM25Index(dir, DefaultBM25Config())
	if err != nil {
		t.Fatalf("NewBM25Index failed: %v", err)
	}
	if idx.totalDocs != 2 {
		t.Errorf("expected 2 documents, got %d", idx.totalDocs)
	}

	// Search
	res := idx.Search("React UI design patterns", 5)
	if len(res) == 0 {
		t.Fatal("expected search results")
	}
	if res[0].Title == "" {
		t.Error("expected non-empty title")
	}

	// Fuzzy search
	resFuzzy := idx.Search("Reactt UI design patternss", 5)
	if len(resFuzzy) == 0 {
		t.Fatal("expected fuzzy search results")
	}

	// Empty query
	resEmpty := idx.Search("the and", 5)
	if len(resEmpty) != 0 {
		t.Errorf("expected 0 results, got %v", resEmpty)
	}

	// topN = 0 (no limit)
	resAll := idx.Search("document", 0)
	if len(resAll) != 2 {
		t.Errorf("expected 2 results with no limit, got %d", len(resAll))
	}
}

func TestExtractBM25Title(t *testing.T) {
	tests := []struct {
		content string
		want    string
	}{
		{"# Hello World\nBody", "Hello World"},
		{"No heading here", ""},
		{"---\ntitle: x\n---\n# My Title\n", "My Title"},
	}
	for _, tc := range tests {
		got := extractBM25Title(tc.content)
		if got != tc.want {
			t.Errorf("extractBM25Title = %q; want %q", got, tc.want)
		}
	}
}

func TestStripYAMLFrontmatter(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"no_frontmatter", "# Hello\nWorld", "# Hello\nWorld"},
		{"with_frontmatter", "---\ntitle: test\n---\n# Hello", "# Hello"},
		{"whitespace_prefix", "  ---\ntitle: test\n---\n# Hello", "# Hello"}, // whitespace-prefixed --- is still treated as frontmatter
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := stripYAMLFrontmatter(tc.content)
			if got != tc.want {
				t.Errorf("stripYAMLFrontmatter = %q; want %q", got, tc.want)
			}
		})
	}
}

func TestTokenize(t *testing.T) {
	idx := &BM25Index{stopwords: defaultStopwords()}
	tokens := idx.tokenize("Hello World the - a big test_case")
	// "the" and "a" are stopwords, "-" stripped
	for _, tok := range tokens {
		if tok == "the" || tok == "a" {
			t.Errorf("stopword %q should be removed", tok)
		}
	}
	if len(tokens) == 0 {
		t.Error("expected some tokens")
	}
}

// ─── engine.go tests ────────────────────────────────────────────────────────

func TestCollectFiles(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("hello"), 0o644)
	_ = os.WriteFile(filepath.Join(dir, "big.md"), []byte(strings.Repeat("x", 2000)), 0o644)
	_ = os.WriteFile(filepath.Join(dir, "code.go"), []byte("package main"), 0o644)
	_ = os.MkdirAll(filepath.Join(dir, ".hidden"), 0o755)
	_ = os.WriteFile(filepath.Join(dir, ".hidden", "file.md"), []byte("hidden"), 0o644)

	// With ext filter and max size
	files, err := CollectFiles(dir, IgnoreConfig{
		SupportedExts:    map[string]bool{".md": true},
		MaxFileSizeBytes: 1500,
	})
	if err != nil {
		t.Fatalf("CollectFiles error: %v", err)
	}
	if len(files) != 1 {
		t.Errorf("expected 1 file, got %d", len(files))
	}

	// Without filter
	allFiles, err := CollectFiles(dir, IgnoreConfig{})
	if err != nil {
		t.Fatalf("CollectFiles error: %v", err)
	}
	// Should include doc.md, big.md, code.go but NOT hidden files
	if len(allFiles) != 3 {
		t.Errorf("expected 3 files, got %d: %v", len(allFiles), allFiles)
	}
}

func TestCollectFilesWithIgnore(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("hello"), 0o644)
	_ = os.MkdirAll(filepath.Join(dir, "vendor"), 0o755)
	_ = os.WriteFile(filepath.Join(dir, "vendor", "lib.md"), []byte("vendored"), 0o644)
	_ = os.WriteFile(filepath.Join(dir, "ignored.log"), []byte("log"), 0o644)

	// Create ignore file
	_ = os.WriteFile(filepath.Join(dir, ".myignore"), []byte("vendor/\n*.log"), 0o644)

	files, err := CollectFiles(dir, IgnoreConfig{
		Filename: ".myignore",
	})
	if err != nil {
		t.Fatalf("CollectFiles error: %v", err)
	}
	// doc.md plus the .myignore file itself survive
	if len(files) != 2 {
		t.Errorf("expected 2 files, got %d: %v", len(files), files)
	}
}

func TestEnsureIgnoreFile(t *testing.T) {
	dir := t.TempDir()

	// Creates file
	EnsureIgnoreFile(dir, IgnoreConfig{Filename: ".myignore"}, "# header")
	data, err := os.ReadFile(filepath.Join(dir, ".myignore"))
	if err != nil {
		t.Fatal("expected ignore file to be created")
	}
	if !strings.Contains(string(data), "# header") {
		t.Error("expected header content")
	}

	// Does not overwrite
	_ = os.WriteFile(filepath.Join(dir, ".myignore"), []byte("custom"), 0o644)
	EnsureIgnoreFile(dir, IgnoreConfig{Filename: ".myignore"}, "# new header")
	data, _ = os.ReadFile(filepath.Join(dir, ".myignore"))
	if string(data) != "custom" {
		t.Error("expected existing file to be preserved")
	}

	// Empty filename
	EnsureIgnoreFile(dir, IgnoreConfig{}, "header")
	// No error, just no-op
}

func TestFindIgnoreFile(t *testing.T) {
	dir := t.TempDir()

	// Not found
	result := findIgnoreFile(dir, ".myignore")
	if result != "" {
		t.Errorf("expected empty, got %q", result)
	}

	// Empty filename
	result = findIgnoreFile(dir, "")
	if result != "" {
		t.Errorf("expected empty for empty filename, got %q", result)
	}

	// Found
	_ = os.WriteFile(filepath.Join(dir, ".myignore"), []byte(""), 0o644)
	result = findIgnoreFile(dir, ".myignore")
	if result == "" {
		t.Error("expected to find ignore file")
	}
}

func TestReadIgnorePatterns(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".ignore")
	_ = os.WriteFile(path, []byte("# comment\n\npattern1\npattern2"), 0o644)

	patterns := readIgnorePatterns(path, []string{"default1"})
	if len(patterns) != 3 {
		t.Errorf("expected 3 patterns, got %d: %v", len(patterns), patterns)
	}

	// Empty path
	patterns = readIgnorePatterns("", []string{"default1"})
	if len(patterns) != 1 {
		t.Errorf("expected 1 default pattern, got %d", len(patterns))
	}
}

func TestMatchIgnore(t *testing.T) {
	tests := []struct {
		relPath  string
		patterns []string
		want     bool
	}{
		{"vendor/lib.go", []string{"vendor/"}, true},
		{"src/vendor/lib.go", []string{"vendor/"}, true},
		{"main.go", []string{"*.go"}, true},
		{"src/main.py", []string{"*.go"}, false},
		{"README.md", []string{""}, false}, // empty pattern
		{"build/out.js", []string{"build/"}, true},
	}
	for _, tc := range tests {
		t.Run(tc.relPath, func(t *testing.T) {
			got := matchIgnore(tc.relPath, tc.patterns)
			if got != tc.want {
				t.Errorf("matchIgnore(%q, %v) = %t; want %t", tc.relPath, tc.patterns, got, tc.want)
			}
		})
	}
}

func TestRunIndexPipeline(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("# Doc\nContent"), 0o644)

	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n:`KDoc`) RETURN n.path AS path, n.content_hash AS hash": {
				Records: []Record{
					{"path": "old.md", "hash": "abc123"},
				},
			},
			"MATCH (n:`KDoc`) RETURN n.path AS path": {
				Records: []Record{
					{"path": "old.md"},
				},
			},
		},
	}

	extractor := &mockExtractor{
		results: map[string]*ExtractionResult{
			"doc.md": {
				Nodes: []Node{{UID: "1", Name: "doc"}},
				Edges: []Edge{{SrcUID: "1", DstUID: "2"}},
			},
		},
	}

	writer := &mockGraphWriter{}

	result, err := RunIndexPipeline(context.Background(), db, writer, extractor, dir, IndexConfig{
		NodeLabels:       []string{"KDoc"},
		EdgeLabels:       []string{"REFS"},
		RootDocNodeLabel: "KDoc",
		IgnoreCfg:        IgnoreConfig{SupportedExts: map[string]bool{".md": true}},
	})
	if err != nil {
		t.Fatalf("RunIndexPipeline error: %v", err)
	}
	if result.IndexedFiles != 1 {
		t.Errorf("expected 1 indexed file, got %d", result.IndexedFiles)
	}
	if result.PrunedFiles != 1 {
		t.Errorf("expected 1 pruned file, got %d", result.PrunedFiles)
	}
}

func TestRunIndexPipelineReset(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("# Doc\nContent"), 0o644)

	db := &mockGraphAdapter{}
	extractor := &mockExtractor{
		results: map[string]*ExtractionResult{
			"doc.md": {Nodes: []Node{{UID: "1", Name: "doc"}}},
		},
	}
	writer := &mockGraphWriter{}

	result, err := RunIndexPipeline(context.Background(), db, writer, extractor, dir, IndexConfig{
		Reset:            true,
		NodeLabels:       []string{"KDoc"},
		RootDocNodeLabel: "KDoc",
		IgnoreCfg:        IgnoreConfig{SupportedExts: map[string]bool{".md": true}},
	})
	if err != nil {
		t.Fatalf("RunIndexPipeline error: %v", err)
	}
	if result.IndexedFiles != 1 {
		t.Errorf("expected 1 indexed, got %d", result.IndexedFiles)
	}
	if result.PrunedFiles != 0 {
		t.Errorf("expected 0 pruned (reset mode), got %d", result.PrunedFiles)
	}
}

func TestRunIndexPipelineSkipUnchanged(t *testing.T) {
	dir := t.TempDir()
	content := "# Doc\nContent"
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte(content), 0o644)

	hash := fmt.Sprintf("%x", [32]byte{})[:16] // won't match
	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n:`KDoc`) RETURN n.path AS path, n.content_hash AS hash": {
				Records: []Record{{"path": "doc.md", "hash": hash}},
			},
		},
	}
	extractor := &mockExtractor{
		results: map[string]*ExtractionResult{
			"doc.md": {Nodes: []Node{{UID: "1"}}},
		},
	}
	writer := &mockGraphWriter{}

	result, err := RunIndexPipeline(context.Background(), db, writer, extractor, dir, IndexConfig{
		RootDocNodeLabel: "KDoc",
		IgnoreCfg:        IgnoreConfig{SupportedExts: map[string]bool{".md": true}},
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	// Should have been indexed since hash doesn't match the real hash
	if result.IndexedFiles != 1 {
		t.Errorf("expected 1 indexed, got %d", result.IndexedFiles)
	}
}

func TestRunIndexPipelineWriteError(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("hello"), 0o644)

	db := &mockGraphAdapter{}
	extractor := &mockExtractor{
		results: map[string]*ExtractionResult{
			"doc.md": {Nodes: []Node{{UID: "1"}}},
		},
	}
	writer := &mockGraphWriter{writeErr: fmt.Errorf("write failed")}

	result, err := RunIndexPipeline(context.Background(), db, writer, extractor, dir, IndexConfig{
		IgnoreCfg: IgnoreConfig{SupportedExts: map[string]bool{".md": true}},
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if result.IndexedFiles != 0 {
		t.Errorf("expected 0 indexed on write error, got %d", result.IndexedFiles)
	}
}

func TestRunIndexPipelineNilExtraction(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("hello"), 0o644)

	db := &mockGraphAdapter{}
	extractor := &mockExtractor{results: map[string]*ExtractionResult{}} // returns nil
	writer := &mockGraphWriter{}

	result, err := RunIndexPipeline(context.Background(), db, writer, extractor, dir, IndexConfig{
		IgnoreCfg: IgnoreConfig{SupportedExts: map[string]bool{".md": true}},
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if result.IndexedFiles != 0 {
		t.Errorf("expected 0 indexed for nil extraction, got %d", result.IndexedFiles)
	}
}

func TestRunIndexPipelineContextCanceled(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "doc.md"), []byte("hello"), 0o644)

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // cancel immediately

	db := &mockGraphAdapter{}
	extractor := &mockExtractor{}
	writer := &mockGraphWriter{}

	_, err := RunIndexPipeline(ctx, db, writer, extractor, dir, IndexConfig{
		IgnoreCfg: IgnoreConfig{SupportedExts: map[string]bool{".md": true}},
	})
	if !errors.Is(err, context.Canceled) {
		t.Errorf("expected context.Canceled, got %v", err)
	}
}

func TestLoadHashes(t *testing.T) {
	ctx := context.Background()

	// Empty root label
	hashes := loadHashes(ctx, &mockGraphAdapter{}, "")
	if len(hashes) != 0 {
		t.Error("expected empty hashes for empty label")
	}

	// Error case
	hashes = loadHashes(ctx, &mockGraphAdapter{queryErr: fmt.Errorf("err")}, "KDoc")
	if len(hashes) != 0 {
		t.Error("expected empty hashes on error")
	}

	// Success with mixed data
	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n:`KDoc`) RETURN n.path AS path, n.content_hash AS hash": {
				Records: []Record{
					{"path": "a.md", "hash": "abc"},
					{"path": "", "hash": "xxx"},   // empty path
					{"path": "b.md", "hash": ""},  // empty hash
					{"path": 123, "hash": "valid"}, // non-string path
				},
			},
		},
	}
	hashes = loadHashes(ctx, db, "KDoc")
	if len(hashes) != 1 || hashes["a.md"] != "abc" {
		t.Errorf("unexpected hashes: %v", hashes)
	}
}

func TestPruneDeleted(t *testing.T) {
	ctx := context.Background()

	// Empty root label
	pruned := pruneDeleted(ctx, &mockGraphAdapter{}, &mockGraphWriter{}, map[string]bool{}, "")
	if pruned != 0 {
		t.Error("expected 0 for empty label")
	}

	// Error case
	pruned = pruneDeleted(ctx, &mockGraphAdapter{queryErr: fmt.Errorf("err")}, &mockGraphWriter{}, map[string]bool{}, "KDoc")
	if pruned != 0 {
		t.Error("expected 0 on error")
	}

	// Prune files
	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n:`KDoc`) RETURN n.path AS path": {
				Records: []Record{
					{"path": "old.md"},
					{"path": "current.md"},
					{"path": ""},
				},
			},
		},
	}
	writer := &mockGraphWriter{}
	current := map[string]bool{"current.md": true}
	pruned = pruneDeleted(ctx, db, writer, current, "KDoc")
	if pruned != 1 {
		t.Errorf("expected 1 pruned, got %d", pruned)
	}
}

func TestDetectCommunities(t *testing.T) {
	ctx := context.Background()

	// Empty graph
	db := &mockGraphAdapter{}
	comms, err := DetectCommunities(ctx, db, IndexConfig{
		NodeLabels: []string{"Node"},
		EdgeLabels: []string{"EDGE"},
	}, AlgoLabelPropagation)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(comms) != 0 {
		t.Error("expected empty communities")
	}

	// Graph with nodes and edges
	db = &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n:`Node`) RETURN n.uid AS uid, n.name AS name": {
				Records: []Record{
					{"uid": "a", "name": "Node A"},
					{"uid": "b", "name": "Node B"},
					{"uid": "c", "name": "Node C"},
				},
			},
			"MATCH (a)-[:EDGE]->(b) RETURN a.uid AS src, b.uid AS dst": {
				Records: []Record{
					{"src": "a", "dst": "b"},
					{"src": "b", "dst": "c"},
				},
			},
		},
	}

	comms, err = DetectCommunities(ctx, db, IndexConfig{
		NodeLabels: []string{"Node"},
		EdgeLabels: []string{"EDGE"},
	}, AlgoLabelPropagation)
	if err != nil {
		t.Fatalf("DetectCommunities error: %v", err)
	}
	if len(comms) == 0 {
		t.Error("expected communities")
	}

	// Test with Louvain
	commsL, err := DetectCommunities(ctx, db, IndexConfig{
		NodeLabels: []string{"Node"},
		EdgeLabels: []string{"EDGE"},
	}, AlgoLouvain)
	if err != nil {
		t.Fatalf("DetectCommunities(Louvain) error: %v", err)
	}
	if len(commsL) == 0 {
		t.Error("expected communities from Louvain")
	}
}

func TestDetectCommunitiesQueryError(t *testing.T) {
	ctx := context.Background()
	db := &mockGraphAdapter{queryErr: fmt.Errorf("err")}
	_, err := DetectCommunities(ctx, db, IndexConfig{
		NodeLabels: []string{"Node"},
		EdgeLabels: []string{"EDGE"},
	}, AlgoLabelPropagation)
	if err != nil {
		t.Fatalf("expected no error from adj loading (query errors are ignored), got: %v", err)
	}
}

func TestGodNodes(t *testing.T) {
	ctx := context.Background()

	// Empty
	gods, err := GodNodes(ctx, &mockGraphAdapter{}, []string{"Node"}, 0)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(gods) != 0 {
		t.Error("expected empty god nodes")
	}

	// With data
	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{},
	}
	// Build the query string that GodNodes uses
	q := fmt.Sprintf(`MATCH (n:`+"`%s`"+`)
			OPTIONAL MATCH (n)-[r]-()
			WITH n, count(r) AS degree
			RETURN n.uid AS uid, n.name AS name, '%s' AS label, degree
			ORDER BY degree DESC LIMIT %d`, "Node", "Node", 20)
	db.queryResults[q] = &QueryResult{
		Records: []Record{
			{"uid": "a", "name": "Node A", "label": "Node", "degree": int64(5)},
			{"uid": "b", "name": "Node B", "label": "Node", "degree": int64(3)},
			{"uid": "", "name": "", "label": "", "degree": int64(0)}, // skipped
		},
	}

	gods, err = GodNodes(ctx, db, []string{"Node"}, 10)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(gods) != 2 {
		t.Errorf("expected 2 god nodes, got %d", len(gods))
	}

	// Test topN limiting — GodNodes uses LIMIT topN*2 in the query, then truncates client-side
	q1 := fmt.Sprintf(`MATCH (n:`+"`%s`"+`)
			OPTIONAL MATCH (n)-[r]-()
			WITH n, count(r) AS degree
			RETURN n.uid AS uid, n.name AS name, '%s' AS label, degree
			ORDER BY degree DESC LIMIT %d`, "Node", "Node", 1*2) // topN*2 = 2
	db.queryResults[q1] = &QueryResult{
		Records: []Record{
			{"uid": "a", "name": "Node A", "label": "Node", "degree": int64(5)},
			{"uid": "b", "name": "Node B", "label": "Node", "degree": int64(3)},
		},
	}
	gods2, err := GodNodes(ctx, db, []string{"Node"}, 1)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(gods2) != 1 {
		t.Errorf("expected 1 god node after limit, got %d", len(gods2))
	}
}

func TestGraphCounts(t *testing.T) {
	ctx := context.Background()

	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n:`Node`) RETURN count(n) AS c": {
				Records: []Record{{"c": int64(10)}},
			},
			"MATCH ()-[r:EDGE]->() RETURN count(r) AS c": {
				Records: []Record{{"c": int64(5)}},
			},
		},
	}

	nodes, edges := GraphCounts(ctx, db, []string{"Node"}, []string{"EDGE"})
	if nodes != 10 {
		t.Errorf("expected 10 nodes, got %d", nodes)
	}
	if edges != 5 {
		t.Errorf("expected 5 edges, got %d", edges)
	}

	// Error case
	dbErr := &mockGraphAdapter{queryErr: fmt.Errorf("err")}
	n, e := GraphCounts(ctx, dbErr, []string{"Node"}, []string{"EDGE"})
	if n != 0 || e != 0 {
		t.Error("expected 0 on error")
	}
}

func TestGuessLabel(t *testing.T) {
	if guessLabel("uid1", []string{"MyLabel"}) != "MyLabel" {
		t.Error("expected first label")
	}
	if guessLabel("uid1", nil) != "Node" {
		t.Error("expected default 'Node'")
	}
}

func TestComputeCohesion(t *testing.T) {
	// Single member
	c := ComputeCohesion(nil, []string{"a"})
	if c != 1.0 {
		t.Errorf("expected 1.0 for single member, got %f", c)
	}

	// Fully connected
	adj := map[string][]string{
		"a": {"b", "c"},
		"b": {"a", "c"},
		"c": {"a", "b"},
	}
	c = ComputeCohesion(adj, []string{"a", "b", "c"})
	if c != 1.0 {
		t.Errorf("expected 1.0 for fully connected, got %f", c)
	}

	// No connections
	adj2 := map[string][]string{
		"a": {},
		"b": {},
	}
	c = ComputeCohesion(adj2, []string{"a", "b"})
	if c != 0.0 {
		t.Errorf("expected 0.0 for disconnected, got %f", c)
	}
}

func TestToInt64(t *testing.T) {
	tests := []struct {
		val  any
		want int64
	}{
		{int64(42), 42},
		{int(10), 10},
		{float64(3.14), 3},
		{"string", 0},
		{nil, 0},
	}
	for _, tc := range tests {
		got := toInt64(tc.val)
		if got != tc.want {
			t.Errorf("toInt64(%v) = %d; want %d", tc.val, got, tc.want)
		}
	}
}

func TestLabelPropagation(t *testing.T) {
	adj := map[string][]string{
		"a": {"b"},
		"b": {"a", "c"},
		"c": {"b"},
		"d": {}, // isolated node
	}
	result := labelPropagation(adj, 50)
	if len(result) != 4 {
		t.Errorf("expected 4 labels, got %d", len(result))
	}
	// a, b, c should share the same community
	if result["a"] != result["b"] || result["b"] != result["c"] {
		t.Error("expected a, b, c in same community")
	}
}

func TestLouvain(t *testing.T) {
	// Empty graph
	result := Louvain(map[string][]string{})
	if len(result) != 0 {
		t.Errorf("expected empty for empty graph")
	}

	// No edges
	result = Louvain(map[string][]string{
		"a": {},
		"b": {},
	})
	if len(result) != 2 {
		t.Error("expected 2 entries for no-edge graph")
	}

	// With edges
	adj := map[string][]string{
		"a": {"b"},
		"b": {"a", "c"},
		"c": {"b"},
	}
	result = Louvain(adj)
	if len(result) != 3 {
		t.Errorf("expected 3 results, got %d", len(result))
	}
}

// ─── generator.go tests ─────────────────────────────────────────────────────

func TestSafeFilename(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"Hello World", "Hello_World"},
		{"File/Path\\To:File", "File-Path-To-File"},
		{"a?b*c", "abc"},
		{"__double__under__", "double_under"},
		{"--double--dash--", "double-dash"},
		{"emoji 🚀 test", "emoji_test"},
	}
	for _, tc := range tests {
		got := SafeFilename(tc.input)
		if got != tc.want {
			t.Errorf("SafeFilename(%q) = %q; want %q", tc.input, got, tc.want)
		}
	}
}

func TestUniqueSlug(t *testing.T) {
	used := make(map[string]bool)
	s1 := uniqueSlug("test", used)
	if s1 != "test" {
		t.Errorf("expected 'test', got %q", s1)
	}
	s2 := uniqueSlug("test", used)
	if s2 != "test_2" {
		t.Errorf("expected 'test_2', got %q", s2)
	}
	s3 := uniqueSlug("test", used)
	if s3 != "test_3" {
		t.Errorf("expected 'test_3', got %q", s3)
	}
}

func TestExtractNameFromUID(t *testing.T) {
	tests := []struct {
		uid  string
		want string
	}{
		{"module::my_func", "my func"},
		{"simple_file.go", "simple_file"},
		{"path/to/file.md", "file"},
		{"no-ext", "no-ext"},
	}
	for _, tc := range tests {
		got := extractNameFromUID(tc.uid)
		if got != tc.want {
			t.Errorf("extractNameFromUID(%q) = %q; want %q", tc.uid, got, tc.want)
		}
	}
}

func TestStrVal(t *testing.T) {
	tests := []struct {
		val  any
		want string
	}{
		{nil, ""},
		{"hello", "hello"},
		{[]byte("bytes"), "bytes"},
		{42, "42"},
	}
	for _, tc := range tests {
		got := strVal(tc.val)
		if got != tc.want {
			t.Errorf("strVal(%v) = %q; want %q", tc.val, got, tc.want)
		}
	}
}

func TestComputeConfidence(t *testing.T) {
	// Full confidence
	full := EntitySummary{
		Name:    "MyEntity",
		Path:    "path/to/file.go",
		Summary: strings.Repeat("x", 200), // > 100 chars
		Type:    "function",
	}
	c := computeConfidence(full)
	if c < 0.9 {
		t.Errorf("expected high confidence, got %f", c)
	}

	// Minimal
	minimal := EntitySummary{}
	c = computeConfidence(minimal)
	if c != 0.0 {
		t.Errorf("expected 0.0 for empty, got %f", c)
	}

	// Type is "document"
	docType := EntitySummary{Name: "x", Type: "document"}
	c = computeConfidence(docType)
	if c >= 0.5 {
		t.Errorf("expected lower confidence for document type, got %f", c)
	}
}

func TestGenericEntityPage(t *testing.T) {
	ent := EntitySummary{
		Name:    "MyEntity",
		Path:    "path/to/file.go",
		Summary: "A short summary",
		Type:    "function",
	}
	page := genericEntityPage(ent, "ast")
	if !strings.Contains(page, "# MyEntity") {
		t.Error("expected entity name in page")
	}
	if !strings.Contains(page, "Provenance") {
		t.Error("expected provenance")
	}
}

func TestGenericEntityPageNoPath(t *testing.T) {
	ent := EntitySummary{
		Name: "MyEntity",
		Type: "function",
	}
	page := genericEntityPage(ent, "ast")
	if strings.Contains(page, "Provenance") {
		t.Error("expected no provenance for empty path")
	}
}

func TestGenericEntityPageNoSummary(t *testing.T) {
	ent := EntitySummary{
		Name: "MyEntity",
		Path: "file.go",
		Type: "function",
	}
	page := genericEntityPage(ent, "ast")
	if strings.Contains(page, "> ") && !strings.Contains(page, "> **") {
		t.Error("expected no summary block")
	}
}

func TestCommunityPage(t *testing.T) {
	c := Community{
		ID:       0,
		Label:    "Test Community",
		Members:  []string{"a", "b"},
		Cohesion: 0.75,
	}
	labels := map[int]string{
		0: "Test Community",
		1: "Other Community",
	}
	page := communityPage(c, labels, "ast")
	if !strings.Contains(page, "# Test Community") {
		t.Error("expected community name")
	}
	if !strings.Contains(page, "[[Other_Community]]") {
		t.Error("expected related communities")
	}
}

func TestCommunityPageNoRelated(t *testing.T) {
	c := Community{ID: 0, Label: "Only", Members: []string{"a"}, Cohesion: 1.0}
	labels := map[int]string{0: "Only"}
	page := communityPage(c, labels, "ast")
	if !strings.Contains(page, "No cross-community connections") {
		t.Error("expected no related communities message")
	}
}

func TestCommunityPageManyMembers(t *testing.T) {
	members := make([]string, 35)
	for i := range members {
		members[i] = fmt.Sprintf("member_%d", i)
	}
	c := Community{ID: 0, Label: "Big", Members: members, Cohesion: 0.5}
	page := communityPage(c, map[int]string{0: "Big"}, "ast")
	if !strings.Contains(page, "and 5 more members") {
		t.Error("expected truncation message")
	}
}

func TestGodNodePage(t *testing.T) {
	gn := map[string]any{
		"name":   "Hub Node",
		"degree": 15,
		"label":  "Function",
	}
	page := godNodePage(gn, map[int]string{}, "ast")
	if !strings.Contains(page, "# Hub Node") {
		t.Error("expected god node name")
	}
	if !strings.Contains(page, "God node") {
		t.Error("expected god node description")
	}
}

func TestGenericIndexPage(t *testing.T) {
	entities := []EntitySummary{
		{Name: "Entity1", Summary: "Short", Type: "function"},
		{Name: "Entity2", Summary: strings.Repeat("x", 100), Type: "function"},
		{Name: "Entity3", Path: "path.go", Type: "class"},
	}
	communities := []Community{
		{Label: "Comm1", Members: []string{"a"}, Cohesion: 0.8},
	}
	godNodes := []map[string]any{
		{"name": "Hub", "degree": 10},
	}

	page := genericIndexPage(entities, communities, godNodes, 10, 5, "ast")
	if !strings.Contains(page, "# Ast Wiki") {
		t.Error("expected index title")
	}
	if !strings.Contains(page, "## Catalog") {
		t.Error("expected catalog section")
	}
	if !strings.Contains(page, "## Communities") {
		t.Error("expected communities section")
	}
	if !strings.Contains(page, "## Hubs") {
		t.Error("expected hubs section")
	}
}

func TestGenericIndexPageEmpty(t *testing.T) {
	page := genericIndexPage(nil, nil, nil, 0, 0, "ast")
	if !strings.Contains(page, "# Ast Wiki") {
		t.Error("expected index title")
	}
	if strings.Contains(page, "## Communities") {
		t.Error("expected no communities section when empty")
	}
	if strings.Contains(page, "## Hubs") {
		t.Error("expected no hubs section when empty")
	}
}

func TestAppendLog(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	// First append - creates file
	appendLog(logPath, "Test Log", 10, 5, 2, 3, "ast")
	data, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal("expected log file to be created")
	}
	content := string(data)
	if !strings.Contains(content, "# Test Log") {
		t.Error("expected log title")
	}
	if !strings.Contains(content, "Nodes: 10") {
		t.Error("expected node count")
	}

	// Second append - appends to existing
	appendLog(logPath, "Test Log", 20, 10, 3, 5, "ast")
	data, _ = os.ReadFile(logPath)
	content = string(data)
	if !strings.Contains(content, "Nodes: 20") {
		t.Error("expected second entry")
	}
}

func TestAppendLogNoDashes(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")
	// Write content without a --- separator
	_ = os.WriteFile(logPath, []byte("# Simple log\nOld content"), 0o644)
	appendLog(logPath, "Test Log", 5, 2, 1, 1, "ast")
	data, _ := os.ReadFile(logPath)
	if !strings.Contains(string(data), "Nodes: 5") {
		t.Error("expected entry appended")
	}
}

func TestLoadEntitySummaries(t *testing.T) {
	ctx := context.Background()

	// Empty query
	result := loadEntitySummaries(ctx, &mockGraphAdapter{}, "")
	if len(result) != 0 {
		t.Error("expected empty for empty query")
	}

	// Error
	result = loadEntitySummaries(ctx, &mockGraphAdapter{queryErr: fmt.Errorf("err")}, "MATCH (n) RETURN n")
	if len(result) != 0 {
		t.Error("expected empty on error")
	}

	// With data
	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n) RETURN n": {
				Records: []Record{
					{"uid": "1", "name": "Entity1", "path": "a.go", "summary": "Sum1", "type": "function"},
					{"uid": "", "name": ""}, // skipped
				},
			},
		},
	}
	result = loadEntitySummaries(ctx, db, "MATCH (n) RETURN n")
	if len(result) != 1 {
		t.Errorf("expected 1 entity, got %d", len(result))
	}
}

func TestGenerateWiki(t *testing.T) {
	dir := t.TempDir()
	wikiDir := filepath.Join(dir, "wiki")

	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n) RETURN n": {
				Records: []Record{
					{"uid": "1", "name": "TestEntity", "path": "a.go", "summary": "Test summary", "type": "function"},
				},
			},
		},
	}

	cfg := WikiConfig{
		OutputDir:       wikiDir,
		EntityQuery:     "MATCH (n) RETURN n",
		NodeLabels:      []string{"Node"},
		EdgeLabels:      []string{"EDGE"},
		Algo:            AlgoLabelPropagation,
		ModuleTag:       "test",
		EnableCrossRefs: true,
	}

	result, err := GenerateWiki(context.Background(), db, cfg)
	if err != nil {
		t.Fatalf("GenerateWiki error: %v", err)
	}
	if result.ArticlesWritten == 0 {
		t.Error("expected articles to be written")
	}

	// Check index was written
	if _, err := os.Stat(filepath.Join(wikiDir, "index.md")); err != nil {
		t.Error("expected index.md")
	}
}

func TestGenerateWikiWithRenderer(t *testing.T) {
	dir := t.TempDir()
	wikiDir := filepath.Join(dir, "wiki")

	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			"MATCH (n) RETURN n": {
				Records: []Record{
					{"uid": "1", "name": "TestEntity", "path": "a.go", "type": "function"},
				},
			},
		},
	}

	renderer := &mockRenderer{
		entityPageContent: "# Custom Entity\nCustom content",
		indexPageContent:  "# Custom Index",
		logTitle:          "Custom Log",
		moduleTag:         "custom",
	}

	cfg := WikiConfig{
		OutputDir:   wikiDir,
		EntityQuery: "MATCH (n) RETURN n",
		NodeLabels:  []string{"Node"},
		Renderer:    renderer,
		ModuleTag:   "custom",
	}

	result, err := GenerateWiki(context.Background(), db, cfg)
	if err != nil {
		t.Fatalf("GenerateWiki error: %v", err)
	}
	if result.ArticlesWritten == 0 {
		t.Error("expected articles written")
	}
}

func TestGenerateWikiWithGodNodes(t *testing.T) {
	dir := t.TempDir()
	wikiDir := filepath.Join(dir, "wiki")

	q := fmt.Sprintf(`MATCH (n:`+"`%s`"+`)
			OPTIONAL MATCH (n)-[r]-()
			WITH n, count(r) AS degree
			RETURN n.uid AS uid, n.name AS name, '%s' AS label, degree
			ORDER BY degree DESC LIMIT %d`, "Node", "Node", 20)

	db := &mockGraphAdapter{
		queryResults: map[string]*QueryResult{
			q: {
				Records: []Record{
					{"uid": "g1", "name": "GodNode", "label": "Node", "degree": int64(10)},
					{"uid": "g2", "name": "", "label": "Node", "degree": int64(5)}, // empty name, skipped
				},
			},
		},
	}

	cfg := WikiConfig{
		OutputDir:  wikiDir,
		NodeLabels: []string{"Node"},
		ModuleTag:  "test",
	}

	result, err := GenerateWiki(context.Background(), db, cfg)
	if err != nil {
		t.Fatalf("GenerateWiki error: %v", err)
	}
	// Should have god node article + index
	if result.ArticlesWritten < 1 {
		t.Error("expected at least 1 article")
	}
}

// ─── export.go tests ────────────────────────────────────────────────────────

func TestExportImport(t *testing.T) {
	wikiDir := t.TempDir()
	_ = os.WriteFile(filepath.Join(wikiDir, "page.md"), []byte("# Page"), 0o644)
	_ = os.MkdirAll(filepath.Join(wikiDir, "subdir"), 0o755)
	_ = os.WriteFile(filepath.Join(wikiDir, "subdir", "nested.md"), []byte("# Nested"), 0o644)

	exportDir := filepath.Join(t.TempDir(), "export")

	// Export
	expResult, err := Export(wikiDir, exportDir, ExportConfig{ModuleTag: "test"})
	if err != nil {
		t.Fatalf("Export error: %v", err)
	}
	if expResult.WikiFiles != 2 {
		t.Errorf("expected 2 wiki files exported, got %d", expResult.WikiFiles)
	}

	// Read manifest
	manifest, err := ReadManifest(exportDir)
	if err != nil {
		t.Fatalf("ReadManifest error: %v", err)
	}
	if manifest.ModuleTag != "test" {
		t.Errorf("expected module tag 'test', got %q", manifest.ModuleTag)
	}

	// Import
	newWikiDir := filepath.Join(t.TempDir(), "imported")
	impResult, err := Import(newWikiDir, exportDir, ImportConfig{})
	if err != nil {
		t.Fatalf("Import error: %v", err)
	}
	if impResult.WikiFiles != 2 {
		t.Errorf("expected 2 wiki files imported, got %d", impResult.WikiFiles)
	}
}

func TestExportNoWikiDir(t *testing.T) {
	exportDir := filepath.Join(t.TempDir(), "export")
	result, err := Export("/nonexistent/wiki", exportDir, ExportConfig{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.WikiFiles != 0 {
		t.Errorf("expected 0 files, got %d", result.WikiFiles)
	}
}

func TestImportNoExport(t *testing.T) {
	wikiDir := t.TempDir()
	result, err := Import(wikiDir, "/nonexistent/export", ImportConfig{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.WikiFiles != 0 {
		t.Errorf("expected 0 files, got %d", result.WikiFiles)
	}
}

func TestReadManifestNotExist(t *testing.T) {
	m, err := ReadManifest(t.TempDir())
	if err != nil {
		t.Fatalf("expected no error for missing manifest, got: %v", err)
	}
	if m.Version != "" {
		t.Error("expected empty manifest")
	}
}

func TestCopyWiki(t *testing.T) {
	src := t.TempDir()
	_ = os.WriteFile(filepath.Join(src, "page.md"), []byte("# Page"), 0o644)

	dst := filepath.Join(t.TempDir(), "dest")
	n, err := CopyWiki(src, dst)
	if err != nil {
		t.Fatalf("CopyWiki error: %v", err)
	}
	if n != 1 {
		t.Errorf("expected 1 file copied, got %d", n)
	}
}

// ─── lint.go tests ──────────────────────────────────────────────────────────

func TestDefaultLintConfig(t *testing.T) {
	cfg := DefaultLintConfig()
	if cfg.StaleDays != 30 {
		t.Errorf("expected 30 stale days, got %d", cfg.StaleDays)
	}
	if cfg.Deep {
		t.Error("expected Deep = false")
	}
}

func TestLintReportHasIssues(t *testing.T) {
	r := &LintReport{Errors: 0}
	if r.HasIssues() {
		t.Error("expected no issues")
	}
	r.Errors = 1
	if !r.HasIssues() {
		t.Error("expected issues")
	}
}

func TestLintReportSummary(t *testing.T) {
	r := &LintReport{TotalPages: 10, Errors: 0}
	s := r.Summary()
	if !strings.Contains(s, "no issues") {
		t.Errorf("unexpected summary: %s", s)
	}
	r.Errors = 3
	s = r.Summary()
	if !strings.Contains(s, "3 issue(s)") {
		t.Errorf("unexpected summary: %s", s)
	}
}

func TestLintWiki(t *testing.T) {
	now := time.Now().UTC().Format("2006-01-02")
	old := time.Now().AddDate(0, -2, 0).Format("2006-01-02")

	pages := map[string]string{
		"index.md":   "# Index\n- [[Page_A]]",
		"log.md":     "# Log",
		"Page_A.md":  fmt.Sprintf("---\ntitle: Page A\ntags: [test]\nupdated: %s\n---\n# Page A\nSee [[Page_B]]\nContent with enough words to pass the empty check and more and more and more text.", now),
		"stale.md":   fmt.Sprintf("---\ntitle: Stale\ntags: [test]\nupdated: %s\n---\n# Stale Page\nContent with enough words to pass the empty check and more and more and more text.", old),
		"empty.md":   fmt.Sprintf("---\ntitle: Empty\ntags: [test]\nupdated: %s\n---\n# Empty\nFew words.", now),
		"no_fm.md":   "# No Frontmatter\nContent with enough words to pass the empty check and more and more and more text.",
		"orphan.md":  fmt.Sprintf("---\ntitle: Orphan\ntags: [test]\nupdated: %s\n---\n# Orphan\nContent with enough words to pass the empty check and more and more and more text.", now),
		"no_date.md": "---\ntitle: No Date\ntags: [test]\n---\n# No Date\nContent with enough words to pass the empty check and more and more and more text.",
	}
	dir := createWikiDir(t, pages)

	report, err := LintWiki(dir, LintConfig{StaleDays: 30})
	if err != nil {
		t.Fatalf("LintWiki error: %v", err)
	}

	if report.Errors == 0 {
		t.Error("expected some lint errors")
	}

	// Test with fix
	report2, err := LintWiki(dir, LintConfig{StaleDays: 30, Fix: true})
	if err != nil {
		t.Fatalf("LintWiki error: %v", err)
	}
	// Fixes should have been applied
	_ = report2
}

func TestLintWikiInvalidDir(t *testing.T) {
	_, err := LintWiki("/nonexistent", LintConfig{})
	if err == nil {
		t.Error("expected error for nonexistent dir")
	}
}

func TestCheckFrontmatter(t *testing.T) {
	// All present
	content := "---\ntitle: test\ntags: [a]\nupdated: 2025-01-01\n---\n# Test"
	missing := checkFrontmatter(content)
	if len(missing) != 0 {
		t.Errorf("expected no missing fields, got %v", missing)
	}

	// No frontmatter
	missing = checkFrontmatter("# No frontmatter")
	if len(missing) != 3 {
		t.Errorf("expected 3 missing fields, got %v", missing)
	}

	// Partial frontmatter
	missing = checkFrontmatter("---\ntitle: test\n---\n# Test")
	if len(missing) != 2 {
		t.Errorf("expected 2 missing fields, got %v", missing)
	}
}

func TestExtractFrontmatter(t *testing.T) {
	tests := []struct {
		content string
		hasData bool
	}{
		{"---\ntitle: test\n---\n# Body", true},
		{"# No frontmatter", false},
		{"  # Not frontmatter", false},
	}
	for _, tc := range tests {
		fm := extractFrontmatter(tc.content)
		if tc.hasData && fm == "" {
			t.Errorf("expected frontmatter for %q", tc.content)
		}
		if !tc.hasData && fm != "" {
			t.Errorf("expected empty frontmatter for %q", tc.content)
		}
	}
}

func TestIsStale(t *testing.T) {
	// No updated field
	if !isStale("---\ntitle: test\n---", 30) {
		t.Error("expected stale for missing updated")
	}

	// Recent
	now := time.Now().UTC().Format("2006-01-02")
	if isStale(fmt.Sprintf("---\nupdated: %s\n---", now), 30) {
		t.Error("expected not stale for recent date")
	}

	// Old
	old := time.Now().AddDate(0, -3, 0).Format("2006-01-02")
	if !isStale(fmt.Sprintf("---\nupdated: %s\n---", old), 30) {
		t.Error("expected stale for old date")
	}

	// Bad date format
	if !isStale("---\nupdated: not-a-date\n---", 30) {
		t.Error("expected stale for bad date")
	}
}

func TestFormatReport(t *testing.T) {
	r := &LintReport{
		TotalPages: 10,
		Errors:     5,
		Orphans:    []string{"orphan1"},
		BrokenLinks: []BrokenLinkInfo{
			{Target: "missing", Source: "page1"},
		},
		StalePages:    []string{"stale1"},
		EmptyPages:    []string{"empty1"},
		MissingFields: []FieldIssue{{Page: "page2", MissingFields: []string{"title"}}},
		FixesApplied:  2,
	}

	formatted := FormatReport(r)
	if !strings.Contains(formatted, "## Orphan Pages") {
		t.Error("expected orphan section")
	}
	if !strings.Contains(formatted, "## Broken Links") {
		t.Error("expected broken links section")
	}
	if !strings.Contains(formatted, "## Stale Pages") {
		t.Error("expected stale section")
	}
	if !strings.Contains(formatted, "## Empty Pages") {
		t.Error("expected empty section")
	}
	if !strings.Contains(formatted, "## Missing Frontmatter") {
		t.Error("expected missing frontmatter section")
	}
	if !strings.Contains(formatted, "Fixes applied") {
		t.Error("expected fixes applied")
	}
}

func TestFormatReportNoIssues(t *testing.T) {
	r := &LintReport{TotalPages: 5, Errors: 0}
	formatted := FormatReport(r)
	if !strings.Contains(formatted, "no issues") {
		t.Error("expected no issues message")
	}
}

// ─── search.go tests ────────────────────────────────────────────────────────

func TestSearchWikiDeduplication(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":   "# Wiki Index\n- [[Page_One]]\n- [[Page_Two]]",
		"Page_One.md": "# Page One\nContent of page one.",
	})

	mockClient := &mockAIClient{
		responses: []string{
			"Page_One",
			"Page_One",
			"DONE: final synthesized answer",
		},
	}

	cfg := SearchConfig{WikiDir: dir, MaxTurns: 3, UseBM25: false}
	res, err := SearchWiki(context.Background(), mockClient, "some query", cfg)
	if err != nil {
		t.Fatalf("SearchWiki failed: %v", err)
	}
	if res.Answer != "final synthesized answer" {
		t.Errorf("expected 'final synthesized answer', got %q", res.Answer)
	}
}

func TestSearchWikiNoIndex(t *testing.T) {
	_, err := SearchWiki(context.Background(), &mockAIClient{}, "query", SearchConfig{
		WikiDir: "/nonexistent",
	})
	if err == nil {
		t.Error("expected error for missing index")
	}
}

func TestSearchWikiAIError(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	_, err := SearchWiki(context.Background(), &errAIClient{}, "query", SearchConfig{
		WikiDir: dir,
	})
	if err == nil {
		t.Error("expected AI error")
	}
}

func TestSearchWikiDONESpace(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	client := &mockAIClient{responses: []string{"DONE This is the answer"}}
	res, err := SearchWiki(context.Background(), client, "query", SearchConfig{WikiDir: dir})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer != "This is the answer" {
		t.Errorf("expected 'This is the answer', got %q", res.Answer)
	}
}

func TestSearchWikiNoPagesFound(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	client := &mockAIClient{responses: []string{"NonExistentPage"}}
	res, err := SearchWiki(context.Background(), client, "query", SearchConfig{WikiDir: dir})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if !strings.Contains(res.Answer, "no matching pages") {
		t.Errorf("expected no matching pages, got %q", res.Answer)
	}
}

func TestSearchWikiImplicitAnswer(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	// Reply that is not a page list and not DONE prefix - interpreted as implicit answer
	client := &mockAIClient{responses: []string{"This is a comprehensive detailed answer with many words."}}
	res, err := SearchWiki(context.Background(), client, "query", SearchConfig{WikiDir: dir})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected implicit answer")
	}
}

func TestSearchWikiPageRefRetry(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	// Reply DONE: with page-ref only answer, then retry returns good answer
	client := &mockAIClient{
		responses: []string{
			"DONE: [[Page_One]]\n[[Page_Two]]",
			"This is a proper synthesized detailed answer.",
		},
	}
	res, err := SearchWiki(context.Background(), client, "query", SearchConfig{WikiDir: dir})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if strings.Contains(res.Answer, "[[Page_One]]") {
		t.Error("expected retry to replace page-ref-only answer")
	}
}

func TestSearchWikiWithBM25(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":   "# Index\n- [[Doc_One]]",
		"Doc_One.md": "# Document One\nContent about testing",
	})
	client := &mockAIClient{responses: []string{"DONE: Answer about testing"}}
	res, err := SearchWiki(context.Background(), client, "testing", SearchConfig{
		WikiDir:  dir,
		UseBM25:  true,
		BM25TopN: 5,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected answer")
	}
}

func TestSearchWikiMaxTurnsExhausted(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":   "# Index",
		"Page_One.md": "# Page One\nContent.",
	})
	// Always request more pages, exhaust turns
	client := &mockAIClient{
		responses: []string{
			"Page_One",
			"Page_One", // duplicate - will add "already loaded" message
			"Page_One",
			"Page_One",
			"Page_One",
			"Page_One",
			"DONE: final answer after synthesis", // final forced synthesis
		},
	}
	res, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 2,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected answer after exhausting turns")
	}
}

func TestSearchWikiFinalAnswerAIError(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":   "# Index",
		"Page_One.md": "# Page One\nContent.",
	})
	callCount := 0
	client := &mockAIClient{
		responses: []string{
			"Page_One", // turn 1
		},
	}
	// Override Complete to fail on second call
	_ = client
	_ = callCount

	// Use a client that succeeds first, then fails
	errClient := &conditionalAIClient{
		failAfter: 2,
		responses: []string{"Page_One"},
	}
	_, err := SearchWiki(context.Background(), errClient, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 1,
	})
	if err == nil {
		t.Error("expected error on final AI call")
	}
}

type conditionalAIClient struct {
	responses []string
	calls     int
	failAfter int
}

func (c *conditionalAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	c.calls++
	if c.calls >= c.failAfter {
		return "", fmt.Errorf("ai error")
	}
	if c.calls <= len(c.responses) {
		return c.responses[c.calls-1], nil
	}
	return "DONE: default", nil
}

func TestSearchWikiFinalPageRefRetry(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":   "# Index",
		"Page_One.md": "# Page One\nContent.",
	})
	// After max turns, final synthesis returns page-ref-only, then retry
	client := &mockAIClient{
		responses: []string{
			"Page_One",
			"[[Page_One]]\n[[Page_Two]]", // final answer = page ref only
			"Proper synthesis answer after retry.",
		},
	}
	res, err := SearchWiki(context.Background(), client, "query", SearchConfig{
		WikiDir:  dir,
		MaxTurns: 1,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer != "Proper synthesis answer after retry." {
		t.Errorf("expected retry answer, got %q", res.Answer)
	}
}

func TestBm25PreFilter(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"doc1.md": "# Document One\nContent about Go programming",
		"doc2.md": "# Document Two\nContent about Python scripting",
	})

	result := bm25PreFilter(dir, "Go programming", 5)
	if result == "" {
		t.Error("expected BM25 pre-filter results")
	}
	if !strings.Contains(result, "BM25 Relevant Pages") {
		t.Error("expected header in BM25 results")
	}

	// Empty dir
	result = bm25PreFilter(t.TempDir(), "query", 5)
	if result != "" {
		t.Error("expected empty result for empty wiki")
	}
}

func TestBM25Search(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"doc1.md": "# Document One\nContent about Go programming and design patterns",
		"doc2.md": "# Document Two\nContent about Python scripting and testing",
	})

	results := BM25Search(dir, "Go programming", 5)
	if len(results) == 0 {
		t.Error("expected search results")
	}
	// Check that snippets were extracted
	for _, r := range results {
		if r.Snippet == "" {
			t.Error("expected snippet")
		}
	}
}

func TestExtractSnippet(t *testing.T) {
	content := "---\ntitle: test\n---\n# Title\nThis is a long document about Go programming. " +
		strings.Repeat("More content. ", 20) +
		"Here we talk about testing patterns."

	// Match found
	snippet := extractSnippet(content, "Go programming")
	if snippet == "" {
		t.Error("expected snippet")
	}
	if !strings.Contains(snippet, "…") && !strings.HasPrefix(snippet, "…") {
		t.Log("snippet does not contain ellipsis — may vary by position")
	}

	// No match - falls back to body start
	snippet = extractSnippet(content, "nonexistentterm")
	if snippet == "" {
		t.Error("expected fallback snippet")
	}

	// Short content
	snippet = extractSnippet("Short body", "nonexistent")
	if snippet != "Short body" {
		t.Errorf("expected 'Short body', got %q", snippet)
	}

	// Match near start
	snippet = extractSnippet("Go programming is great", "Go")
	if snippet == "" {
		t.Error("expected snippet for match at start")
	}
}

func TestParsePageList(t *testing.T) {
	tests := []struct {
		reply string
		want  int
	}{
		{"Page_One\nPage_Two", 2},
		{"- Page_One\n* Page_Two", 2},
		{"1. Page_One\n2. Page_Two", 2},
		{"3. Page_Three\n4. Page_Four\n5. Page_Five", 3},
		{"DONE: answer", 0},
		{"some/path/thing", 0}, // contains /
		{"[[Page_One]]\n[[Page_Two]]", 2},
		{"Page_One.md", 1},
		{"", 0},
	}
	for _, tc := range tests {
		t.Run(tc.reply, func(t *testing.T) {
			got := parsePageList(tc.reply)
			if len(got) != tc.want {
				t.Errorf("parsePageList(%q) returned %d pages; want %d: %v", tc.reply, len(got), tc.want, got)
			}
		})
	}
}

func TestLoadWikiPage(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"Page_One.md": "# Page One\nContent",
	})

	// Exact match
	content, slug := loadWikiPage(dir, "Page_One")
	if content == "" || slug != "Page_One" {
		t.Errorf("expected exact match, got slug %q", slug)
	}

	// SafeFilename match
	content, slug = loadWikiPage(dir, "Page One")
	if content == "" || slug != "Page_One" {
		t.Errorf("expected SafeFilename match, got slug %q", slug)
	}

	// Fuzzy match
	content, slug = loadWikiPage(dir, "PageOne")
	if content == "" || slug != "Page_One" {
		t.Errorf("expected fuzzy match, got slug %q", slug)
	}

	// No match
	content, slug = loadWikiPage(dir, "CompletelyDifferent")
	if content != "" || slug != "" {
		t.Error("expected no match")
	}
}

func TestFindBestFuzzyMatch(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"Page_One.md":       "# Page One",
		"index.md":          "# Index",
		"log.md":            "# Log",
		"Another_Page.md":   "# Another",
		"notmarkdown.txt":   "text",
	})

	// Good match
	match := findBestFuzzyMatch(dir, "PageOne")
	if match != "Page_One" {
		t.Errorf("expected 'Page_One', got %q", match)
	}

	// No match
	match = findBestFuzzyMatch(dir, "XYZ_ABC_DEF")
	if match != "" {
		t.Errorf("expected empty, got %q", match)
	}

	// Empty target
	match = findBestFuzzyMatch(dir, "")
	if match != "" {
		t.Error("expected empty for empty target")
	}

	// Invalid dir
	match = findBestFuzzyMatch("/nonexistent", "test")
	if match != "" {
		t.Error("expected empty for invalid dir")
	}
}

func TestCleanForFuzzy(t *testing.T) {
	got := cleanForFuzzy("Hello World! 123")
	if got != "helloworld123" {
		t.Errorf("cleanForFuzzy = %q; want 'helloworld123'", got)
	}
}

func TestGetTrigrams(t *testing.T) {
	// Short string
	tg := getTrigrams("ab")
	if !tg["ab"] || len(tg) != 1 {
		t.Errorf("expected single trigram for short string, got %v", tg)
	}

	// Normal string
	tg = getTrigrams("hello")
	if len(tg) != 3 { // hel, ell, llo
		t.Errorf("expected 3 trigrams, got %d: %v", len(tg), tg)
	}
}

func TestTrigramSimilarity(t *testing.T) {
	// Identical
	s := trigramSimilarity("hello", "hello")
	if s != 1.0 {
		t.Errorf("expected 1.0 for identical, got %f", s)
	}

	// Empty strings
	s = trigramSimilarity("", "")
	if s != 1.0 { // both have same single trigram ""
		t.Errorf("expected 1.0 for empty strings, got %f", s)
	}

	// Different
	s = trigramSimilarity("hello", "world")
	if s >= 1.0 {
		t.Error("expected < 1.0 for different strings")
	}
}

func TestBuildSearchSystemPrompt(t *testing.T) {
	prompt := buildSearchSystemPrompt("knowledge")
	if !strings.Contains(prompt, "knowledge") {
		t.Error("expected module tag in prompt")
	}
}

// ─── multi_search.go tests ──────────────────────────────────────────────────

func TestSearchMultiWikiNoSources(t *testing.T) {
	_, err := SearchMultiWiki(context.Background(), &mockAIClient{}, "query", MultiWikiSearchConfig{})
	if err == nil {
		t.Error("expected error for no sources")
	}
}

func TestSearchMultiWikiSingleSource(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	client := &mockAIClient{responses: []string{"DONE: single source answer"}}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: []WikiSource{{ID: "src1", Label: "Source 1", Dir: dir}},
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer != "single source answer" {
		t.Errorf("expected 'single source answer', got %q", res.Answer)
	}
}

func TestSearchMultiWikiMultiSources(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{
		"index.md":   "# Source 1 Index\n- [[Page_A]]",
		"Page_A.md":  "# Page A\nContent about testing",
	})
	dir2 := createWikiDir(t, map[string]string{
		"index.md":   "# Source 2 Index\n- [[Page_B]]",
		"Page_B.md":  "# Page B\nContent about design",
	})

	client := &mockAIClient{
		responses: []string{
			"[src1]/Page_A",
			"DONE: Multi-source answer synthesized from both.",
		},
	}

	sources := []WikiSource{
		{ID: "src1", Label: "Source 1", Dir: dir1},
		{ID: "src2", Label: "Source 2", Dir: dir2},
	}

	res, err := SearchMultiWiki(context.Background(), client, "testing", MultiWikiSearchConfig{
		Sources:  sources,
		MaxTurns: 3,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected answer")
	}
}

func TestSearchMultiWikiDONESpace(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md": "# Index",
	})
	client := &mockAIClient{responses: []string{"DONE answer with space prefix"}}
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir},
		{ID: "s2", Label: "S2", Dir: t.TempDir()},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: sources,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer != "answer with space prefix" {
		t.Errorf("expected 'answer with space prefix', got %q", res.Answer)
	}
}

func TestSearchMultiWikiNoPagesFound(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{"index.md": "# Index"})
	dir2 := createWikiDir(t, map[string]string{"index.md": "# Index"})
	client := &mockAIClient{responses: []string{"[src1]/NonExistent"}}
	sources := []WikiSource{
		{ID: "src1", Label: "S1", Dir: dir1},
		{ID: "src2", Label: "S2", Dir: dir2},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: sources,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if !strings.Contains(res.Answer, "no matching pages") {
		t.Errorf("expected no matching pages, got %q", res.Answer)
	}
}

func TestSearchMultiWikiImplicitAnswer(t *testing.T) {
	dir := createWikiDir(t, map[string]string{"index.md": "# Index"})
	client := &mockAIClient{responses: []string{"This is a comprehensive answer."}}
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir},
		{ID: "s2", Label: "S2", Dir: t.TempDir()},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: sources,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected implicit answer")
	}
}

func TestSearchMultiWikiAIError(t *testing.T) {
	dir := createWikiDir(t, map[string]string{"index.md": "# Index"})
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir},
		{ID: "s2", Label: "S2", Dir: t.TempDir()},
	}
	_, err := SearchMultiWiki(context.Background(), &errAIClient{}, "query", MultiWikiSearchConfig{
		Sources: sources,
	})
	if err == nil {
		t.Error("expected AI error")
	}
}

func TestSearchMultiWikiWithBM25(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{
		"index.md":  "# Index",
		"page1.md":  "# Page 1\nContent about Go programming",
	})
	dir2 := createWikiDir(t, map[string]string{
		"index.md":  "# Index",
		"page2.md":  "# Page 2\nContent about Python scripting",
	})
	client := &mockAIClient{responses: []string{"DONE: answer"}}
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir1},
		{ID: "s2", Label: "S2", Dir: dir2},
	}
	res, err := SearchMultiWiki(context.Background(), client, "Go programming", MultiWikiSearchConfig{
		Sources:           sources,
		UseBM25:           true,
		BM25TopNPerSource: 3,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected answer")
	}
}

func TestSearchMultiWikiMaxTurnsExhausted(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{
		"index.md":  "# Index",
		"Page_A.md": "# Page A\nContent",
	})
	dir2 := createWikiDir(t, map[string]string{"index.md": "# Index"})
	client := &mockAIClient{
		responses: []string{
			"[src1]/Page_A",
			"[src1]/Page_A", // duplicate
			"Final forced synthesis answer",
		},
	}
	sources := []WikiSource{
		{ID: "src1", Label: "S1", Dir: dir1},
		{ID: "src2", Label: "S2", Dir: dir2},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources:  sources,
		MaxTurns: 2,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer == "" {
		t.Error("expected answer")
	}
}

func TestSearchMultiWikiFinalAIError(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":  "# Index",
		"Page_A.md": "# Page A\nContent",
	})
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir},
		{ID: "s2", Label: "S2", Dir: t.TempDir()},
	}
	client := &conditionalAIClient{
		failAfter: 2,
		responses: []string{"[s1]/Page_A"},
	}
	_, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources:  sources,
		MaxTurns: 1,
	})
	if err == nil {
		t.Error("expected final AI error")
	}
}

func TestSearchMultiWikiPageRefRetry(t *testing.T) {
	dir := createWikiDir(t, map[string]string{"index.md": "# Index"})
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir},
		{ID: "s2", Label: "S2", Dir: t.TempDir()},
	}
	client := &mockAIClient{
		responses: []string{
			"DONE: [[Page_A]]\n[[Page_B]]", // page ref only
			"Good synthesized answer.",
		},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: sources,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if strings.Contains(res.Answer, "[[Page_A]]") {
		t.Error("expected retry to replace page-ref answer")
	}
}

func TestSearchMultiWikiFinalPageRefRetry(t *testing.T) {
	dir := createWikiDir(t, map[string]string{
		"index.md":  "# Index",
		"Page_A.md": "# Page A\nContent",
	})
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir},
		{ID: "s2", Label: "S2", Dir: t.TempDir()},
	}
	client := &mockAIClient{
		responses: []string{
			"[s1]/Page_A",
			"[[Page_A]]\n[[Page_B]]", // final = page ref only
			"Proper final synthesis.",
		},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources:  sources,
		MaxTurns: 1,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer != "Proper final synthesis." {
		t.Errorf("expected retry answer, got %q", res.Answer)
	}
}

func TestBM25SearchMulti(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{
		"doc1.md": "# Document One\nContent about Golang programming language patterns",
	})
	dir2 := createWikiDir(t, map[string]string{
		"doc2.md": "# Document Two\nContent about Python scripting language patterns",
	})
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir1},
		{ID: "s2", Label: "S2", Dir: dir2},
	}
	results := BM25SearchMulti(sources, "Golang programming", 5)
	if len(results) == 0 {
		t.Error("expected results")
	}
	foundS1 := false
	for _, r := range results {
		if r.SourceID == "s1" {
			foundS1 = true
		}
	}
	if !foundS1 {
		t.Error("expected result from s1")
	}
}

func TestBm25PreFilterMulti(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{
		"doc1.md": "# Doc 1\nContent about Go programming",
	})
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: dir1},
	}
	result := bm25PreFilterMulti(sources, "Go programming", 5)
	if result == "" {
		t.Error("expected BM25 multi pre-filter results")
	}

	// Empty
	result = bm25PreFilterMulti(nil, "query", 5)
	if result != "" {
		t.Error("expected empty for no sources")
	}
}

func TestBuildMultiSearchSystemPrompt(t *testing.T) {
	sources := []WikiSource{
		{ID: "s1", Label: "Source 1"},
		{ID: "s2", Label: "Source 2"},
	}
	prompt := buildMultiSearchSystemPrompt(sources)
	if !strings.Contains(prompt, "[s1]") || !strings.Contains(prompt, "Source 1") {
		t.Error("expected source info in prompt")
	}
}

func TestParseMultiPageList(t *testing.T) {
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "Fallback_Page.md"), []byte("# Fallback"), 0o644)

	sources := []WikiSource{
		{ID: "src1", Label: "S1", Dir: t.TempDir()},
		{ID: "src2", Label: "S2", Dir: dir},
	}

	tests := []struct {
		name  string
		reply string
		want  int
	}{
		{"bracket_slash", "[src1]/Page_A", 1},
		{"no_bracket", "src1/Page_A", 1},
		{"wikilink_format", "- [src1]/[[Page_A]]", 1},
		{"done_line", "DONE: answer", 0},
		{"empty", "", 0},
		{"numbered", "1. [src1]/Page_A\n2. [src2]/Page_B", 2},
		// Fallback: page without source prefix
		{"fallback", "Fallback_Page", 1},
		// Invalid: contains : or /
		{"invalid_colon", "http://example.com", 0},
		// Asterisk list
		{"asterisk", "* [src1]/Page_X", 1},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := parseMultiPageList(tc.reply, sources)
			if len(got) != tc.want {
				t.Errorf("parseMultiPageList(%q) = %d; want %d", tc.reply, len(got), tc.want)
			}
		})
	}
}

func TestParseMultiPageListNumbered(t *testing.T) {
	sources := []WikiSource{
		{ID: "s1", Label: "S1", Dir: t.TempDir()},
	}
	// Test all numbered prefixes 1-9
	var lines []string
	for i := 1; i <= 9; i++ {
		lines = append(lines, fmt.Sprintf("%d. [s1]/Page_%d", i, i))
	}
	reply := strings.Join(lines, "\n")
	got := parseMultiPageList(reply, sources)
	if len(got) != 9 {
		t.Errorf("expected 9 requests, got %d", len(got))
	}
}

func TestSearchMultiWikiDuplicatePages(t *testing.T) {
	dir1 := createWikiDir(t, map[string]string{
		"index.md":  "# Index",
		"Page_A.md": "# Page A\nContent",
	})
	dir2 := createWikiDir(t, map[string]string{"index.md": "# Index"})
	sources := []WikiSource{
		{ID: "src1", Label: "S1", Dir: dir1},
		{ID: "src2", Label: "S2", Dir: dir2},
	}

	client := &mockAIClient{
		responses: []string{
			"[src1]/Page_A",
			"[src1]/Page_A", // duplicate, already loaded
			"DONE: final answer",
		},
	}
	res, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources:  sources,
		MaxTurns: 3,
	})
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if res.Answer != "final answer" {
		t.Errorf("expected 'final answer', got %q", res.Answer)
	}
}
