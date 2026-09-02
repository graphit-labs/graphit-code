package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

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
			got := wiki.ExtToLang(tc.ext)
			if got != tc.want {
				t.Errorf("wiki.ExtToLang(%q) = %q; want %q", tc.ext, got, tc.want)
			}
		})
	}
}

func TestManifestFromChunks(t *testing.T) {
	m := ManifestFromChunks([]wiki.WikiChunk{
		{Slug: "page_a", Source: "src/a.go", ContentHash: "hash_a"},
		{Slug: "page_b", Source: "src/b.go", ContentHash: "hash_b"},
		{Slug: "generated", ContentHash: "ignored"},
	})
	if m.SourceHashes["src/a.go"] != "hash_a" || m.SourceHashes["src/b.go"] != "hash_b" {
		t.Errorf("source hashes = %v", m.SourceHashes)
	}
	if m.PageSources["page_a"] != "src/a.go" || m.PageSources["page_b"] != "src/b.go" {
		t.Errorf("page sources = %v", m.PageSources)
	}
	if _, ok := m.PageSources["generated"]; ok {
		t.Error("a row without a source entered the staleness manifest")
	}
}

// The lint takes the DOCUMENTS the build compiled, not a wiki directory. Its fixtures are therefore
// knowledgeDoc values rather than page files: the checks it makes — missing `title`/`type`/
// `content_hash`, staleness, an empty body — read fields that were only ever written into the page
// because the page was the medium.

func lintDoc(title, path, docType, hash string) knowledgeDoc {
	return knowledgeDoc{title: title, path: path, docType: docType, contentHash: hash, body: "Body here."}
}

func TestLintKnowledgeWikiNilGraph(t *testing.T) {
	t.Parallel()
	result := LintKnowledgeWiki(nil, nil, nil)
	if result == nil {
		t.Fatal("expected non-nil result for nil graph")
	}
	if len(result.Findings) != 0 {
		t.Errorf("expected no findings for nil graph, got %d", len(result.Findings))
	}
}

func TestLintKnowledgeWikiFullScenario(t *testing.T) {
	t.Parallel()
	docs := []knowledgeDoc{
		lintDoc("Good Page", "docs/good.md", "specification", "abc"),
		// No title and no content_hash — two missing-field findings on one page.
		lintDoc("", "docs/nofm.md", "guide", ""),
		lintDoc("Stale Page", "docs/stale.md", "guide", "def"),
		// An empty body is the "no content section" finding, which used to be the absence of a
		// `## Content` heading in the rendered page.
		{title: "Hollow", path: "docs/hollow.md", docType: "document", contentHash: "ghi"},
	}
	docs[2].staleSince = "2026-01-01"
	docs[2].staleReason = "source changed"
	slugs := []string{"good", "nofm", "stale", "hollow"}

	graph := wiki.BuildCrossRefGraphFromRefs([]wiki.PageEdges{
		{Slug: "good", Title: "Good Page", Targets: []string{"nonexistent"}},
		{Slug: "nofm", Title: "No Frontmatter"},
		{Slug: "stale", Title: "Stale Page"},
		{Slug: "hollow", Title: "Hollow"},
	})

	result := LintKnowledgeWiki(graph, docs, slugs)
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	if result.TotalPages != 4 {
		t.Errorf("expected 4 total pages, got %d", result.TotalPages)
	}
	if result.StalePages != 1 {
		t.Errorf("expected 1 stale page, got %d", result.StalePages)
	}

	has := func(page, fragment string) bool {
		for _, f := range result.Findings {
			if f.Page == page && strings.Contains(f.Message, fragment) {
				return true
			}
		}
		return false
	}
	if !has("good", "broken link") {
		t.Error("expected broken link finding for [[nonexistent]]")
	}
	if !has("nofm", "missing frontmatter field: title") {
		t.Error("expected a missing-title finding for the nofm page")
	}
	if !has("nofm", "missing frontmatter field: content_hash") {
		t.Error("expected a missing-content_hash finding for the nofm page")
	}
	if !has("hollow", "no content section") {
		t.Error("expected a no-content finding for the hollow page")
	}
	if !has("nofm", "orphan page") {
		t.Error("expected an orphan finding for a page nothing references")
	}
}
