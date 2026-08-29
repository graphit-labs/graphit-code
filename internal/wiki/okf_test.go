package wiki

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// A markdown link is not evidence of a cross-reference. Since the move off [[wikilinks]],
// the Provenance line on every generated page and every body link into the source tree are
// plain markdown links too — and ResolveSlug flattens them into slugs that exist nowhere.
// This is the test that keeps 354 phantom "broken links" from coming back.
func TestIsBundlePageLinkRejectsPathsAndFragments(t *testing.T) {
	t.Parallel()
	pages := []string{
		"Storage_Layout",
		"Storage_Layout.md",
		"/Storage_Layout.md", // bundle-relative, the form OKF §6.1 recommends
		"wiki://Storage_Layout",
		"graphit.lock.json_handling", // a dotted slug is still a slug
	}
	notPages := []string{
		"docs/tasks/okf-wiki-compliance.md",
		"../../internal/ast/pipeline.go",
		"./other/page.md",
		"internal/wiki/store.go",
		"#section-heading",
		"https://example.com/page.md",
		"http://example.com",
		"mailto:someone@example.com",
		"",
		"   ",
	}
	for _, l := range pages {
		if !isBundlePageLink(l) {
			t.Errorf("isBundlePageLink(%q) = false; it addresses a page of this wiki", l)
		}
	}
	for _, l := range notPages {
		if isBundlePageLink(l) {
			t.Errorf("isBundlePageLink(%q) = true; it does not address a page of this wiki", l)
		}
	}
}

func TestBuildCrossRefGraphIgnoresProvenanceLinks(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	page := "---\ntype: document\ntitle: A\n---\n\n# A\n\n" +
		"*Provenance: [docs/specs/a.md](docs/specs/a.md)*\n\n" +
		"See [B](B.md) and the code in [pipeline.go](../../internal/ast/pipeline.go).\n"
	if err := os.WriteFile(filepath.Join(dir, "A.md"), []byte(page), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "B.md"), []byte("---\ntype: document\n---\n\n# B\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	graph, err := BuildCrossRefGraph(dir)
	if err != nil {
		t.Fatal(err)
	}
	out := graph.Outbound["A"]
	if len(out) != 1 || out[0] != "B" {
		t.Errorf("outbound = %v; want exactly [B] — a repo path is not a page", out)
	}
	if broken := BrokenLinks(graph); len(broken) != 0 {
		t.Errorf("broken links = %v; want none", broken)
	}
}

func TestWriteOKFGeneratedIsAMappingWithAnActor(t *testing.T) {
	t.Parallel()
	var b strings.Builder
	WriteOKFGenerated(&b, OKFActor("knowledge"), "2026-08-29")
	got := b.String()

	// §5.2: `generated` is a mapping and `generated.by` is REQUIRED inside it. A flat key
	// literally named `generated.at` is the spec's prose notation, not a field.
	if strings.HasPrefix(got, "generated.at:") {
		t.Fatalf("emitted the dotted key that is not an OKF field: %q", got)
	}
	if !strings.Contains(got, "by:") || !strings.Contains(got, "at: 2026-08-29") {
		t.Errorf("generated = %q; want a mapping carrying both by and at", got)
	}
	// §5.3 derives the trust tier from the `human:` prefix, so a generated page must not
	// claim one.
	if strings.Contains(got, "human:") {
		t.Errorf("a generated page must not claim a human actor: %q", got)
	}
}

func TestWriteOKFSourcesEmitsResourceEntries(t *testing.T) {
	t.Parallel()
	var b strings.Builder
	WriteOKFSources(&b, "docs/specs/a.md", "", "  ")
	got := b.String()

	// §5.1: each entry is a mapping whose `resource` is REQUIRED. A bare string is not one.
	if got != "sources:\n  - resource: docs/specs/a.md\n" {
		t.Errorf("sources = %q", got)
	}

	var empty strings.Builder
	WriteOKFSources(&empty, "", "   ")
	if empty.String() != "" {
		t.Errorf("no resources should emit no sources key; got %q", empty.String())
	}
}

func TestAppendOKFLogEntriesGroupsByDate(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")

	if err := AppendOKFLogEntries(logPath, "Test Log", "2026-08-28",
		[]LogEntry{{Kind: LogCreation, Text: "First."}}); err != nil {
		t.Fatal(err)
	}
	if err := AppendOKFLogEntries(logPath, "Test Log", "2026-08-29",
		[]LogEntry{{Kind: LogUpdate, Text: "Second."}}); err != nil {
		t.Fatal(err)
	}
	if err := AppendOKFLogEntries(logPath, "Test Log", "2026-08-29",
		[]LogEntry{{Kind: LogUpdate, Text: "Third."}}); err != nil {
		t.Fatal(err)
	}

	data, _ := os.ReadFile(logPath)
	got := string(data)

	if strings.HasPrefix(strings.TrimSpace(got), "---") {
		t.Error("a §9 log carries no frontmatter")
	}
	if n := strings.Count(got, "## 2026-08-29"); n != 1 {
		t.Errorf("same-day entries must share one heading; got %d", n)
	}
	// §9: newest first, and within a day the newest entry leads.
	iNew, iOld := strings.Index(got, "## 2026-08-29"), strings.Index(got, "## 2026-08-28")
	if iNew < 0 || iOld < 0 || iNew > iOld {
		t.Errorf("newest date must come first:\n%s", got)
	}
	if strings.Index(got, "Third.") > strings.Index(got, "Second.") {
		t.Errorf("newest entry of the day must lead:\n%s", got)
	}
	if !strings.Contains(got, "* **Creation**: First.") {
		t.Errorf("entry kind lost:\n%s", got)
	}
}

func TestAppendOKFLogEntriesKeepsExistingHistoryBelow(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	logPath := filepath.Join(dir, "log.md")
	if err := AppendOKFLogEntries(logPath, "Log", "2026-08-20",
		[]LogEntry{{Kind: LogCreation, Text: "Older."}}); err != nil {
		t.Fatal(err)
	}
	if err := AppendOKFLogEntries(logPath, "Log", "2026-08-29",
		[]LogEntry{{Kind: LogUpdate, Text: "Newer."}}); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(logPath)
	got := string(data)

	if !strings.Contains(got, "Older.") {
		t.Error("a log is append-only history; earlier entries must survive")
	}
	if strings.Index(got, "Newer.") > strings.Index(got, "Older.") {
		t.Errorf("newest first:\n%s", got)
	}
}

// A search hands the agent titles so it can choose; the page itself is a separate,
// deliberate call to the source tool. Nothing in a default hit is page text.
func TestFormatSearchResultsTOONIsTitlesByDefault(t *testing.T) {
	t.Parallel()
	results := []WikiSearchResult{{
		Slug: "Storage_Layout", Title: "Storage Layout", DocType: "architecture", Score: 8.5,
		Summary: strings.Repeat("body text ", 60),
	}}

	plain := FormatSearchResultsTOON(results, false)
	if strings.Contains(plain, "body text") {
		t.Errorf("a default hit must carry no page text:\n%s", plain)
	}
	if !strings.Contains(plain, "results[1]{slug|title|type|score}:") {
		t.Errorf("unexpected header:\n%s", plain)
	}

	withPreview := FormatSearchResultsTOON(results, true)
	if !strings.Contains(withPreview, "body text") {
		t.Errorf("preview:true must include an excerpt:\n%s", withPreview)
	}
	if len(withPreview) >= len(strings.Repeat("body text ", 60)) {
		t.Errorf("the excerpt must be bounded, not the whole summary:\n%s", withPreview)
	}
}

func TestFormatBM25ResultsTOONIsTitlesByDefault(t *testing.T) {
	t.Parallel()
	results := []BM25Result{{
		Path: "A_Memory.md", Title: "A Memory", DocType: "correction", Score: 3.25,
		Snippet: "What: something long enough to matter",
	}}

	plain := FormatBM25ResultsTOON(results, false)
	if strings.Contains(plain, "something long enough") {
		t.Errorf("a default hit must carry no memory text:\n%s", plain)
	}
	// The slug is what wiki_source takes, so the `.md` must not travel with it.
	if !strings.Contains(plain, "  A_Memory|A Memory|correction|3.2") {
		t.Errorf("unexpected row:\n%s", plain)
	}
	if !strings.Contains(FormatBM25ResultsTOON(results, true), "something long enough") {
		t.Error("preview:true must include an excerpt")
	}
}
