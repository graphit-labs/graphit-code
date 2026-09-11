package wiki

import (
	"sort"
	"strings"
	"testing"
)

func refs(slug, title string, targets ...string) PageEdges {
	return PageEdges{Slug: slug, Title: title, Targets: targets}
}

func TestBuildCrossRefGraphFromRefs(t *testing.T) {
	t.Parallel()
	graph := BuildCrossRefGraphFromRefs([]PageEdges{
		refs("alpha", "Alpha", "beta", "gamma"),
		refs("beta", "Beta", "alpha"),
		refs("gamma", "Gamma"),
	})

	if len(graph.AllPages) != 3 {
		t.Errorf("AllPages = %d, want 3", len(graph.AllPages))
	}

	outAlpha := graph.Outbound["alpha"]
	sort.Strings(outAlpha)
	if len(outAlpha) != 2 {
		t.Fatalf("alpha outbound = %v, want 2 entries", outAlpha)
	}
	if outAlpha[0] != "beta" || outAlpha[1] != "gamma" {
		t.Errorf("alpha outbound = %v, want [beta gamma]", outAlpha)
	}

	if len(graph.Inbound["beta"]) != 1 || graph.Inbound["beta"][0] != "alpha" {
		t.Errorf("beta inbound = %v, want [alpha]", graph.Inbound["beta"])
	}

	if graph.Titles["alpha"] != "Alpha" {
		t.Errorf("alpha title = %q, want Alpha", graph.Titles["alpha"])
	}
}

func TestBuildCrossRefGraphFromRefs_DedupLinks(t *testing.T) {
	t.Parallel()
	graph := BuildCrossRefGraphFromRefs([]PageEdges{
		refs("page", "Page", "target", "target"),
		refs("target", "Target"),
	})

	if len(graph.Outbound["page"]) != 1 {
		t.Errorf("expected deduplicated outbound, got %v", graph.Outbound["page"])
	}
}

func TestBuildCrossRefGraphFromRefs_SelfLinkIgnored(t *testing.T) {
	t.Parallel()
	graph := BuildCrossRefGraphFromRefs([]PageEdges{refs("self", "Self", "self")})

	if len(graph.Outbound["self"]) != 0 {
		t.Errorf("self-links should be ignored, got %v", graph.Outbound["self"])
	}
}

func TestBuildCrossRefGraphFromRefs_NoTitleFallsBackToSlug(t *testing.T) {
	t.Parallel()
	graph := BuildCrossRefGraphFromRefs([]PageEdges{refs("notitle", "")})

	if graph.Titles["notitle"] != "notitle" {
		t.Errorf("title = %q, want 'notitle' (slug fallback)", graph.Titles["notitle"])
	}
}

func TestBuildCrossRefGraphFromRefs_UnknownTargetIsABrokenLink(t *testing.T) {
	t.Parallel()
	graph := BuildCrossRefGraphFromRefs([]PageEdges{refs("page", "Page", "missing")})

	if len(graph.Outbound["page"]) != 1 || graph.Outbound["page"][0] != "missing" {
		t.Fatalf("outbound = %v, want [missing]", graph.Outbound["page"])
	}
	if stats := CrossRefStats(graph); stats.BrokenLinks != 1 {
		t.Errorf("BrokenLinks = %d, want 1", stats.BrokenLinks)
	}
}

func TestCrossRefStats(t *testing.T) {
	t.Parallel()
	graph := BuildCrossRefGraphFromRefs([]PageEdges{
		refs("alpha", "Alpha", "beta"),
		refs("beta", "Beta"),
		refs("lonely", "Lonely"),
	})

	stats := CrossRefStats(graph)
	if stats.TotalPages != 3 {
		t.Errorf("TotalPages = %d, want 3", stats.TotalPages)
	}
	if stats.TotalLinks != 1 {
		t.Errorf("TotalLinks = %d, want 1", stats.TotalLinks)
	}
	if stats.BacklinksAdded != 1 {
		t.Errorf("BacklinksAdded = %d, want 1", stats.BacklinksAdded)
	}
	if stats.OrphanPages != 1 {
		t.Errorf("OrphanPages = %d, want 1", stats.OrphanPages)
	}
	if stats.BrokenLinks != 0 {
		t.Errorf("BrokenLinks = %d, want 0", stats.BrokenLinks)
	}
}

func TestCrossRefStatsNilGraph(t *testing.T) {
	t.Parallel()
	if stats := CrossRefStats(nil); stats.TotalPages != 0 {
		t.Errorf("a nil graph must report nothing, got %+v", stats)
	}
}

func TestOrphanPages(t *testing.T) {
	t.Parallel()
	graph := &CrossRefGraph{
		AllPages: map[string]bool{"orphan": true, "connected": true},
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
		{"md_links", "See [alpha](alpha.md) and [beta](beta.md).", []string{"alpha", "beta"}},
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
		{"with_md_ext", "hello.md", "hello"},
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
			"Main content\n## Backlinks\n\n- [link1](link1.md)\n- [link2](link2.md)",
			"Main content",
		},
		{
			"no_backlinks",
			"Main content only",
			"Main content only",
		},
		{
			"starts_with_backlinks",
			"## Backlinks\n- [link1](link1.md)",
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
	if !strings.Contains(result, "[Source One](source1.md) — Source One") {
		t.Error("expected source1 with title")
	}
	if !strings.Contains(result, "[source2](source2.md)") {
		t.Error("expected source2")
	}
	if strings.Contains(result, "[source2](source2.md) — source2") {
		t.Error("should not append title when title equals slug")
	}
}

func TestInjectBacklinksSection_ReplacesExisting(t *testing.T) {
	t.Parallel()
	content := "# Page\nContent.\n## Backlinks\n\n- [old](old.md)"
	titles := map[string]string{"new": "New Page"}

	result := injectBacklinksSection(content, []string{"new"}, titles)

	if strings.Contains(result, "[old](old.md)") {
		t.Error("old backlinks should be replaced")
	}
	if !strings.Contains(result, "(new.md)") {
		t.Error("new backlinks should be present")
	}
}
