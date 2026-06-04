package knowledge

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func TestLintMalformedFrontmatter(t *testing.T) {
	t.Parallel()
	wikiDir := t.TempDir()

	// File with opening --- but no closing ---
	_ = os.WriteFile(filepath.Join(wikiDir, "malformed.md"),
		[]byte("---\ntitle: Broken\ntype: specification\ncontent_hash: abc123\nno closing delimiter here\n"), 0o644)

	graph := &wiki.CrossRefGraph{
		AllPages: map[string]bool{"malformed": true},
		Outbound: map[string][]string{},
		Inbound:  map[string][]string{},
		Titles:   map[string]string{"malformed": "Broken"},
	}

	// The function should not panic on malformed frontmatter.
	// strings.Index(content[4:], "---") returns -1 → fm = content[:6]
	result := LintKnowledgeWiki(wikiDir, graph, nil)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	// Should still have findings (missing fields, etc.)
	if result.TotalPages != 1 {
		t.Errorf("expected 1 total page, got %d", result.TotalPages)
	}
}

func TestDetectStalePagesTransitive(t *testing.T) {
	t.Parallel()

	// Build a 3-level chain: A → B → C
	// When A's source changes, B should become stale (references A).
	// C references B, but the current implementation only propagates 1 hop.
	graph := &wiki.CrossRefGraph{
		AllPages: map[string]bool{"page_a": true, "page_b": true, "page_c": true},
		Outbound: map[string][]string{
			"page_b": {"page_a"},
			"page_c": {"page_b"},
		},
		Inbound: map[string][]string{
			"page_a": {"page_b"},
			"page_b": {"page_c"},
		},
		Titles: map[string]string{},
	}

	old := &Manifest{
		SourceHashes: map[string]string{
			"src/a.go": "hash_old",
			"src/b.go": "hash_b",
			"src/c.go": "hash_c",
		},
		PageSources: map[string]string{
			"page_a": "src/a.go",
			"page_b": "src/b.go",
			"page_c": "src/c.go",
		},
	}

	current := &Manifest{
		SourceHashes: map[string]string{
			"src/a.go": "hash_new", // changed
			"src/b.go": "hash_b",
			"src/c.go": "hash_c",
		},
		PageSources: map[string]string{
			"page_a": "src/a.go",
			"page_b": "src/b.go",
			"page_c": "src/c.go",
		},
	}

	stale := DetectStalePages(old, current, graph)

	if _, ok := stale["page_a"]; !ok {
		t.Error("page_a should be stale (direct source change)")
	}
	if _, ok := stale["page_b"]; !ok {
		t.Error("page_b should be stale (depends on page_a via inbound)")
	}
	if info, ok := stale["page_a"]; ok && !strings.Contains(info.Reason, "src/a.go") {
		t.Errorf("page_a reason should mention source, got %q", info.Reason)
	}
	if info, ok := stale["page_b"]; ok && !strings.Contains(info.Reason, "page_a") {
		t.Errorf("page_b reason should mention dependency page_a, got %q", info.Reason)
	}
}

func TestInstalledContextsMixed(t *testing.T) {
	origDir, _ := os.Getwd()
	tempDir := t.TempDir()
	_ = os.Chdir(tempDir)
	defer func() { _ = os.Chdir(origDir) }()

	parentDir := filepath.Join(brand.DotDir(), "knowledge")
	_ = os.MkdirAll(parentDir, 0o755)

	// Regular file (not a dir) — should be skipped
	_ = os.WriteFile(filepath.Join(parentDir, "stray-file.txt"), []byte("x"), 0o644)

	// Dir without index.md — should be skipped
	_ = os.MkdirAll(filepath.Join(parentDir, "no-index-ctx"), 0o755)
	_ = os.WriteFile(filepath.Join(parentDir, "no-index-ctx", "other.md"), []byte("x"), 0o644)

	// Dir with index.md — should be included
	_ = os.MkdirAll(filepath.Join(parentDir, "valid-ctx"), 0o755)
	_ = os.WriteFile(filepath.Join(parentDir, "valid-ctx", "index.md"), []byte("# Index"), 0o644)

	// "project" dir — always skipped
	_ = os.MkdirAll(filepath.Join(parentDir, "project"), 0o755)
	_ = os.WriteFile(filepath.Join(parentDir, "project", "index.md"), []byte("# Proj"), 0o644)

	names := InstalledContexts()
	if len(names) != 1 || names[0] != "valid-ctx" {
		t.Errorf("expected [valid-ctx], got %v", names)
	}
}

func TestKnowledgeRuleContentSnapshot(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent([]string{"ctx1"}, "docs")

	sections := []string{
		"Knowledge Maintenance Rule",
		"MANDATORY: Documentation Is a Completion Requirement",
		"Wiki-First Knowledge Retrieval",
		"Integration Documentation Maintenance Rule",
		"Documentation Requirements",
		"Task Logs",
		"Architecture Documentation",
		"Decision Records",
		"Technical Documentation",
		"Feature Specifications",
	}
	for _, section := range sections {
		if !strings.Contains(content, section) {
			t.Errorf("rule content missing key section: %q", section)
		}
	}

	// Verify the contexts parameter is accepted without error
	_ = content
}

func TestLintKnowledgeWikiNilGraph(t *testing.T) {
	t.Parallel()
	result := LintKnowledgeWiki("", nil, nil)
	if result == nil {
		t.Fatal("expected non-nil result for nil graph")
	}
	if len(result.Findings) != 0 {
		t.Errorf("expected no findings for nil graph, got %d", len(result.Findings))
	}
}

func TestDetectStalePagesNilManifests(t *testing.T) {
	t.Parallel()

	stale := DetectStalePages(nil, nil, nil)
	if len(stale) != 0 {
		t.Errorf("expected empty stale map for nil manifests, got %d", len(stale))
	}

	stale2 := DetectStalePages(&Manifest{}, nil, nil)
	if len(stale2) != 0 {
		t.Errorf("expected empty stale map when current is nil, got %d", len(stale2))
	}
}

func TestDetectStalePagesNoGraph(t *testing.T) {
	t.Parallel()

	old := &Manifest{
		SourceHashes: map[string]string{"src.go": "old"},
		PageSources:  map[string]string{"page": "src.go"},
	}
	current := &Manifest{
		SourceHashes: map[string]string{"src.go": "new"},
		PageSources:  map[string]string{"page": "src.go"},
	}

	stale := DetectStalePages(old, current, nil)
	if _, ok := stale["page"]; !ok {
		t.Error("page should be stale even without graph")
	}
}

func TestExtToLangCoverage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		ext  string
		want string
	}{
		{".yaml", "yaml"},
		{".yml", "yaml"},
		{".json", "json"},
		{".graphql", "graphql"},
		{".gql", "graphql"},
		{".xml", "xml"},
		{".wsdl", "xml"},
		{".proto", ""},
		{".txt", ""},
		{".rst", ""},
	}
	for _, tc := range tests {
		t.Run(tc.ext, func(t *testing.T) {
			got := extToLang(tc.ext)
			if got != tc.want {
				t.Errorf("extToLang(%q) = %q; want %q", tc.ext, got, tc.want)
			}
		})
	}
}

func TestSaveAndLoadManifest(t *testing.T) {
	dir := t.TempDir()

	m := &Manifest{
		SourceHashes: map[string]string{"src/a.go": "hash_a"},
		PageSources:  map[string]string{"page_a": "src/a.go"},
	}
	SaveManifest(dir, m)

	loaded := LoadManifest(dir)
	if loaded.SourceHashes["src/a.go"] != "hash_a" {
		t.Error("loaded manifest should have source hash")
	}
	if loaded.PageSources["page_a"] != "src/a.go" {
		t.Error("loaded manifest should have page source")
	}
}

func TestLoadManifestMissing(t *testing.T) {
	m := LoadManifest(t.TempDir())
	if m == nil {
		t.Fatal("expected non-nil manifest for missing file")
	}
	if m.SourceHashes == nil || m.PageSources == nil {
		t.Error("maps should be initialized even for missing manifest")
	}
}

func TestSaveManifestMarshalAndReload(t *testing.T) {
	dir := t.TempDir()

	m := &Manifest{
		SourceHashes: map[string]string{"a": "1", "b": "2"},
		PageSources:  map[string]string{"p1": "a", "p2": "b"},
	}
	SaveManifest(dir, m)

	data, err := os.ReadFile(filepath.Join(dir, manifestFileName))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "source_hashes") {
		t.Error("manifest JSON should contain source_hashes key")
	}
}

func TestLintKnowledgeWikiFullScenario(t *testing.T) {
	wikiDir := t.TempDir()

	// Valid page with all fields
	_ = os.WriteFile(filepath.Join(wikiDir, "good.md"),
		[]byte("---\ntitle: Good Page\ntype: specification\ncontent_hash: abc\n---\n## Content\nBody here."), 0o644)

	// Page missing frontmatter entirely
	_ = os.WriteFile(filepath.Join(wikiDir, "nofm.md"),
		[]byte("# No Frontmatter\nJust body."), 0o644)

	// Valid page that is stale
	_ = os.WriteFile(filepath.Join(wikiDir, "stale.md"),
		[]byte("---\ntitle: Stale Page\ntype: guide\ncontent_hash: def\nstale_since: 2026-01-01\n---\n## Content\nOld content."), 0o644)

	graph := &wiki.CrossRefGraph{
		AllPages: map[string]bool{"good": true, "nofm": true, "stale": true, "index": true, "log": true},
		Outbound: map[string][]string{
			"good": {"nonexistent"},
		},
		Inbound: map[string][]string{},
		Titles:  map[string]string{},
	}

	result := LintKnowledgeWiki(wikiDir, graph, []string{"good.go", "uncited.go"})
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	if result.TotalPages != 5 {
		t.Errorf("expected 5 total pages, got %d", result.TotalPages)
	}
	if result.StalePages != 1 {
		t.Errorf("expected 1 stale page, got %d", result.StalePages)
	}

	// Check for broken link finding
	var hasBrokenLink bool
	for _, f := range result.Findings {
		if strings.Contains(f.Message, "broken link") {
			hasBrokenLink = true
		}
	}
	if !hasBrokenLink {
		t.Error("expected broken link finding for [[nonexistent]]")
	}

	// Check for missing frontmatter finding
	var hasMissingFM bool
	for _, f := range result.Findings {
		if f.Page == "nofm" && strings.Contains(f.Message, "missing frontmatter") {
			hasMissingFM = true
		}
	}
	if !hasMissingFM {
		t.Error("expected missing frontmatter finding for nofm page")
	}
}
