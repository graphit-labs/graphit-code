package wiki

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

func TestBuildCrossRefGraph(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "alpha.md", "# Alpha\nSee [[beta]] and [[gamma]].")
	writeFile(t, dir, "beta.md", "# Beta\nReferences [[alpha]].")
	writeFile(t, dir, "gamma.md", "# Gamma\nStandalone page.")

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(graph.AllPages) != 3 {
		t.Errorf("AllPages = %d, want 3", len(graph.AllPages))
	}

	// alpha links to beta and gamma
	outAlpha := graph.Outbound["alpha"]
	sort.Strings(outAlpha)
	if len(outAlpha) != 2 {
		t.Fatalf("alpha outbound = %v, want 2 entries", outAlpha)
	}
	if outAlpha[0] != "beta" || outAlpha[1] != "gamma" {
		t.Errorf("alpha outbound = %v, want [beta gamma]", outAlpha)
	}

	// beta has inbound from alpha
	if len(graph.Inbound["beta"]) != 1 || graph.Inbound["beta"][0] != "alpha" {
		t.Errorf("beta inbound = %v, want [alpha]", graph.Inbound["beta"])
	}

	// Titles
	if graph.Titles["alpha"] != "Alpha" {
		t.Errorf("alpha title = %q, want Alpha", graph.Titles["alpha"])
	}
}

func TestBuildCrossRefGraph_DedupLinks(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "page.md", "# Page\n[[target]] and again [[target]].")
	writeFile(t, dir, "target.md", "# Target\nContent.")

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(graph.Outbound["page"]) != 1 {
		t.Errorf("expected deduplicated outbound, got %v", graph.Outbound["page"])
	}
}

func TestBuildCrossRefGraph_SelfLinkIgnored(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "self.md", "# Self\n[[self]] reference.")

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(graph.Outbound["self"]) != 0 {
		t.Errorf("self-links should be ignored, got %v", graph.Outbound["self"])
	}
}

func TestBuildCrossRefGraph_NoH1Title(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "notitle.md", "No heading here, just text.")

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if graph.Titles["notitle"] != "notitle" {
		t.Errorf("title = %q, want 'notitle' (slug fallback)", graph.Titles["notitle"])
	}
}

func TestBuildCrossRefGraphInvalidDir(t *testing.T) {
	t.Parallel()
	_, err := BuildCrossRefGraph(filepath.Join(t.TempDir(), "nonexistent"))
	if err == nil {
		t.Error("expected error for non-existent dir")
	}
}

func TestInjectBacklinks(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "alpha.md", "# Alpha\nSee [[beta]].")
	writeFile(t, dir, "beta.md", "# Beta\nSome content.")

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	result, err := InjectBacklinks(dir, graph)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if result.BacklinksAdded == 0 {
		t.Error("expected at least one backlink to be added")
	}

	data, err := os.ReadFile(filepath.Join(dir, "beta.md"))
	if err != nil {
		t.Fatalf("reading beta.md: %v", err)
	}
	if !strings.Contains(string(data), "## Backlinks") {
		t.Error("expected backlinks section in beta.md")
	}
	if !strings.Contains(string(data), "[[alpha]]") {
		t.Error("expected [[alpha]] in beta.md backlinks")
	}
}

func TestInjectBacklinks_SkipsIndexAndLog(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "index.md", "# Index\n[[page]]")
	writeFile(t, dir, "log.md", "# Log\n[[page]]")
	writeFile(t, dir, "page.md", "# Page\nContent.")

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	_, err = InjectBacklinks(dir, graph)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// page should have backlinks, but index and log should not be modified
	data, _ := os.ReadFile(filepath.Join(dir, "page.md"))
	if !strings.Contains(string(data), "## Backlinks") {
		t.Error("expected backlinks in page.md")
	}
}

func TestInjectBacklinks_Idempotent(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "a.md", "# A\n[[b]]")
	writeFile(t, dir, "b.md", "# B\nContent.")

	graph, _ := BuildCrossRefGraph(dir)
	_, _ = InjectBacklinks(dir, graph)

	// Run again — should detect no change
	graph2, _ := BuildCrossRefGraph(dir)
	result2, _ := InjectBacklinks(dir, graph2)

	if result2.BacklinksAdded != 0 {
		t.Errorf("expected 0 additional backlinks on second run, got %d", result2.BacklinksAdded)
	}
}

func TestOrphanPages(t *testing.T) {
	t.Parallel()
	graph := &CrossRefGraph{
		AllPages: map[string]bool{"orphan": true, "connected": true, "index": true, "log": true},
		Outbound: map[string][]string{"connected": {"somewhere"}},
		Inbound:  map[string][]string{},
	}

	orphans := OrphanPages(graph)
	if len(orphans) != 1 || orphans[0] != "orphan" {
		t.Errorf("orphans = %v, want [orphan]", orphans)
	}
}

func TestOrphanPages_NoOrphans(t *testing.T) {
	t.Parallel()
	graph := &CrossRefGraph{
		AllPages: map[string]bool{"a": true, "b": true},
		Outbound: map[string][]string{"a": {"b"}},
		Inbound:  map[string][]string{"b": {"a"}},
	}

	orphans := OrphanPages(graph)
	if len(orphans) != 0 {
		t.Errorf("expected no orphans, got %v", orphans)
	}
}

func TestBrokenLinks(t *testing.T) {
	t.Parallel()
	graph := &CrossRefGraph{
		AllPages: map[string]bool{"source": true},
		Outbound: map[string][]string{"source": {"missing_page", "also_missing"}},
		Inbound:  map[string][]string{},
	}

	broken := BrokenLinks(graph)
	if len(broken) != 2 {
		t.Fatalf("expected 2 broken links, got %d", len(broken))
	}

	targets := []string{broken[0].Target, broken[1].Target}
	sort.Strings(targets)
	if targets[0] != "also_missing" || targets[1] != "missing_page" {
		t.Errorf("broken targets = %v, want [also_missing missing_page]", targets)
	}
}

func TestBrokenLinks_NoBroken(t *testing.T) {
	t.Parallel()
	graph := &CrossRefGraph{
		AllPages: map[string]bool{"a": true, "b": true},
		Outbound: map[string][]string{"a": {"b"}},
		Inbound:  map[string][]string{},
	}

	broken := BrokenLinks(graph)
	if len(broken) != 0 {
		t.Errorf("expected no broken links, got %v", broken)
	}
}

func TestBrokenLinks_Dedup(t *testing.T) {
	t.Parallel()
	graph := &CrossRefGraph{
		AllPages: map[string]bool{"a": true, "b": true},
		Outbound: map[string][]string{
			"a": {"missing"},
			"b": {"missing"},
		},
		Inbound: map[string][]string{},
	}

	broken := BrokenLinks(graph)
	if len(broken) != 1 {
		t.Errorf("expected deduplicated broken links, got %d", len(broken))
	}
}

func TestFindWikiLinks(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		want    []string
	}{
		{"basic", "See [[alpha]] and [[beta]].", []string{"alpha", "beta"}},
		{"with_pipe_alias", "See [[alpha|Alpha Page]].", []string{"alpha"}},
		{"in_code_block", "```\n[[should_ignore]]\n```\n[[visible]]", []string{"visible"}},
		{"inline_code", "This `[[ignored]]` and [[visible]].", []string{"visible"}},
		{"no_links", "No wikilinks here.", nil},
		{"dedup", "[[same]] and [[same]] again.", []string{"same"}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := FindWikiLinks(tt.content)
			if len(got) != len(tt.want) {
				t.Errorf("FindWikiLinks() = %v, want %v", got, tt.want)
				return
			}
			for i := range got {
				if got[i] != tt.want[i] {
					t.Errorf("FindWikiLinks()[%d] = %q, want %q", i, got[i], tt.want[i])
				}
			}
		})
	}
}

func TestResolveSlug(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		raw  string
		want string
	}{
		{"plain", "hello", "hello"},
		{"with_pipe", "page|display name", "page"},
		{"with_spaces", "Hello World", "Hello_World"},
		{"empty", "", ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := ResolveSlug(tt.raw)
			if got != tt.want {
				t.Errorf("ResolveSlug(%q) = %q, want %q", tt.raw, got, tt.want)
			}
		})
	}
}

func TestStripCodeBlocks(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			"fenced_block",
			"before\n```\nhidden code\n```\nafter\n",
			"before\nafter\n\n",
		},
		{
			"inline_code",
			"text `hidden` more text\n",
			"text  more text\n\n",
		},
		{
			"no_code",
			"plain text\n",
			"plain text\n\n",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := stripCodeBlocks(tt.content)
			if got != tt.want {
				t.Errorf("stripCodeBlocks() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestStripBacklinksSection(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			"with_backlinks",
			"Main content\n## Backlinks\n\n- [[link1]]\n- [[link2]]",
			"Main content",
		},
		{
			"no_backlinks",
			"Main content only",
			"Main content only",
		},
		{
			"starts_with_backlinks",
			"## Backlinks\n- [[link1]]",
			"",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := stripBacklinksSection(tt.content)
			if got != tt.want {
				t.Errorf("stripBacklinksSection() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestInjectBacklinksSection(t *testing.T) {
	t.Parallel()
	titles := map[string]string{
		"source1": "Source One",
		"source2": "source2",
	}

	result := injectBacklinksSection("# Page\nContent.", []string{"source1", "source2"}, titles)

	if !strings.Contains(result, "## Backlinks") {
		t.Error("expected backlinks header")
	}
	if !strings.Contains(result, "[[source1]] — Source One") {
		t.Error("expected source1 with title")
	}
	if !strings.Contains(result, "[[source2]]") {
		t.Error("expected source2")
	}
	// source2 has slug == title, so should NOT have " — " suffix
	if strings.Contains(result, "[[source2]] — source2") {
		t.Error("should not append title when title equals slug")
	}
}

func TestInjectBacklinksSection_ReplacesExisting(t *testing.T) {
	t.Parallel()
	content := "# Page\nContent.\n## Backlinks\n\n- [[old]]"
	titles := map[string]string{"new": "New Page"}

	result := injectBacklinksSection(content, []string{"new"}, titles)

	if strings.Contains(result, "[[old]]") {
		t.Error("old backlinks should be replaced")
	}
	if !strings.Contains(result, "[[new]]") {
		t.Error("new backlinks should be present")
	}
}
