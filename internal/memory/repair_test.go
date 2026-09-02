package memory

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// The fork this repairs: a write path recovered a memory id from the FILE NAME, so
// `<ulid>_important_.md` became a memory whose id was `<ulid>_important_`. It then accumulated
// revisions of its own and compiled as a second page for one memory, which search answered twice.
//
// Measured in this repository's project scope before the fix: 496 files for 312 ids.

// indexedMemoryPages reports how many live and superseded rows the compiled index holds.
//
// It replaced globbing `*.md` in the wiki directory: pages are not written any more, and the
// index is what a search can answer with.
func indexedMemoryPages(t *testing.T, wikiDir string) (live, superseded int, ids []string) {
	t.Helper()
	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("opening the memory index: %v", err)
	}
	defer func() { _ = db.Close() }()

	entries, err := db.Browse(context.Background(), wiki.BrowseFilter{ClusterID: -1, Limit: 10000})
	if err != nil {
		t.Fatalf("browsing the memory index: %v", err)
	}
	for _, e := range entries {
		res, err := db.Search(context.Background(), e.Title, 50)
		if err != nil {
			continue
		}
		for _, r := range res {
			if r.Slug != e.Slug {
				continue
			}
			ids = append(ids, r.EntityID)
			if r.Superseded {
				superseded++
			} else {
				live++
			}
			break
		}
	}
	return live, superseded, ids
}

func TestIsForkedMemoryFileName(t *testing.T) {
	tests := []struct {
		name string
		want bool
	}{
		{"01ARZ3NDEKTSV4RRFFQ69G5FAV.md", false},
		{"01ARZ3NDEKTSV4RRFFQ69G5FAV_important_.md", true},
		{"01ARZ3NDEKTSV4RRFFQ69G5FAV_2.md", true},
		{"MEM1.md", false},
		{"index.md", false},
		{"a-hand-written-fixture-name-that-is-long.md", false},
	}
	for _, tc := range tests {
		if got := isForkedMemoryFileName(tc.name); got != tc.want {
			t.Errorf("isForkedMemoryFileName(%q) = %v, want %v", tc.name, got, tc.want)
		}
	}
}

// A search result budget must count memories, not index rows: collapsing a chain after ranking
// would otherwise silently shrink the answer.
func TestTopKCountsDistinctMemories(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	for _, title := range []string{"alpha", "beta", "gamma"} {
		id, err := svc.AddMemory(title, "quokka marker "+title+" first", MemoryOpts{})
		if err != nil {
			t.Fatal(err)
		}
		for _, pass := range []string{"second", "third"} {
			if err := svc.UpdateMemory(id, title, "quokka marker "+title+" "+pass); err != nil {
				t.Fatal(err)
			}
		}
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	compileFromTable(t, svc, wikiDir)

	results := SearchChains(context.Background(), wikiDir, "quokka marker", 3)
	if len(results) != 3 {
		t.Fatalf("got %d results for top_k 3 over 3 chains of 3 revisions, want 3", len(results))
	}
	seen := map[string]bool{}
	for _, r := range results {
		if r.Superseded {
			t.Errorf("result %s is a superseded revision while its current one also matched", r.Path)
		}
		if seen[r.MemoryID] {
			t.Errorf("memory %s appears twice", r.MemoryID)
		}
		seen[r.MemoryID] = true
	}
}

// The formatter stays silent about the chain when nothing in the answer is superseded, so the
// common case costs exactly what it did before.
func TestChainTOONOmitsTheChainColumnsWhenNothingIsSuperseded(t *testing.T) {
	plain := []ChainResult{{BM25Result: wiki.BM25Result{Path: "a.md", Title: "A", DocType: "fact", Score: 1}}}
	if got := FormatChainResultsTOON(plain, false); strings.Contains(got, "superseded") {
		t.Errorf("the header carries chain columns for an answer with no superseded hit:\n%s", got)
	}

	mixed := []ChainResult{plain[0], {
		BM25Result: wiki.BM25Result{Path: "b.md", Title: "B", DocType: "fact", Score: 0.5},
		Superseded: true,
		Current:    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
	}}
	got := FormatChainResultsTOON(mixed, false)
	if !strings.Contains(got, "superseded|current") {
		t.Errorf("the header is missing the chain columns:\n%s", got)
	}
	if !strings.Contains(got, "01ARZ3NDEKTSV4RRFFQ69G5FAV") {
		t.Errorf("the current memory id is not in the output:\n%s", got)
	}
}

// A title is a free-text sentence, and a sentence containing ": " is not valid YAML unquoted.
// Measured in this repository's project scope: 47 files whose frontmatter therefore parsed to
// NOTHING — their type, importance and tags invisible to search, listing and consolidation alike.
func TestFrontmatterWithAnUnquotedColonStillParses(t *testing.T) {
	content := "---\n" +
		"id: 01ARZ3NDEKTSV4RRFFQ69G5FAV\n" +
		"title: Telemetria do Hub: eventos vão para refs/events/*, nunca para uma branch\n" +
		"scope: project\n" +
		"scope_id: p\n" +
		"type: fact\n" +
		"important: true\n" +
		"tags: [memory, project, fact, hub]\n" +
		"---\n\n# Telemetria do Hub\n\nbody\n"

	fm, ok := ParseMemoryFrontmatterOK(content)
	if !ok {
		t.Fatal("the recovering parse failed on an unquoted colon in the title")
	}
	if fm.ID != "01ARZ3NDEKTSV4RRFFQ69G5FAV" {
		t.Errorf("id = %q", fm.ID)
	}
	if !strings.HasPrefix(fm.Title, "Telemetria do Hub:") {
		t.Errorf("title = %q, want the whole sentence including the colon", fm.Title)
	}
	if fm.Type != "fact" || !fm.Important {
		t.Errorf("type = %q important = %v, want fact/true — classification was lost", fm.Type, fm.Important)
	}
	if len(fm.Tags) != 4 {
		t.Errorf("tags = %v, want four", fm.Tags)
	}
}

// The guard that matters more than the recovery: when a frontmatter genuinely cannot be read, a
// write must not replace it with an empty one. That trade is how a full memory becomes a valid
// file with no type, no tags and no timestamps, reported as a success.
func TestAnUnreadableFrontmatterIsNeverRewrittenEmpty(t *testing.T) {
	// A tab-indented mapping under a scalar cannot be recovered by quoting.
	broken := "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\n\ttitle: x\n  - y\n---\n\n# Recoverable Title\n\nbody\n"

	if _, ok := ParseMemoryFrontmatterOK(broken); ok {
		t.Skip("this fixture is parseable, so it cannot exercise the guard")
	}

	if got := withImportantFlag(broken, true); got != broken {
		t.Errorf("promotion rewrote an unreadable memory:\n%s", got)
	}

	updated := updatedMemoryContent(broken, memoryUpdate{
		ID: "01ARZ3NDEKTSV4RRFFQ69G5FAV", Scope: "project", ScopeID: "p", NewBody: "new body",
	})
	if fm := ParseMemoryFrontmatter(updated); fm.Title != "Recoverable Title" {
		t.Errorf("title = %q, want the H1 recovered from the body", fm.Title)
	}
	if !strings.Contains(updated, "new body") {
		t.Error("the update lost the new body")
	}
}

func TestQuoteUnquotedScalarsLeavesGoodLinesAlone(t *testing.T) {
	block := "id: 01ARZ3NDEKTSV4RRFFQ69G5FAV\n" +
		"title: 'already quoted: fine'\n" +
		"important: true\n" +
		"revision: 3\n" +
		"tags: [memory, project]\n"

	got := quoteUnquotedScalars(block)
	for _, want := range []string{
		"title: 'already quoted: fine'",
		"important: true",
		"revision: 3",
		"tags: [memory, project]",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("line %q was altered:\n%s", want, got)
		}
	}
}

// Chain collapse must be answerable from the index alone. It used to open each hit's page and
// parse its frontmatter — a file read per hit for something the columns can project.
//
// The test proves it by making the pages unreadable after the index is built: if the search still
// resolves the chain, it did not touch them.
func TestChainResolvesFromTheIndexAndNotFromThePages(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("Column-resolved", "the marker word wombat and an old detail", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Column-resolved", "the marker word wombat and a new detail"); err != nil {
		t.Fatal(err)
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	compileFromTable(t, svc, wikiDir)

	// Blank every page. The index keeps its columns; the frontmatter is gone.
	pages, err := filepath.Glob(filepath.Join(wikiDir, "*.md"))
	if err != nil {
		t.Fatal(err)
	}
	for _, p := range pages {
		if err := os.WriteFile(p, []byte("gutted\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	results := SearchChains(context.Background(), wikiDir, "wombat", 10)
	if len(results) != 1 {
		for _, r := range results {
			t.Logf("hit %s entity=%s superseded=%v", r.Path, r.MemoryID, r.Superseded)
		}
		t.Fatalf("got %d results, want 1 — the chain was not resolved from the index", len(results))
	}
	if results[0].MemoryID != id {
		t.Errorf("memory_id = %q, want %q", results[0].MemoryID, id)
	}
	if results[0].Superseded {
		t.Error("the surviving result is the superseded revision")
	}
}

// The columns are generic, so a wiki that is not memory can carry supersession too — an ADR
// replaced by a later one, a spec kept for reference.
func TestSupersessionColumnsRoundTripThroughTheIndex(t *testing.T) {
	wikiDir := filepath.Join(t.TempDir(), "wiki")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	chunks := []wiki.WikiChunk{
		{
			Slug: "adr-0007-current", Title: "ADR 7", Body: "we index quokka tables in lance",
			DocType: "decision", EntityID: "adr-0007", WordCount: 6,
		},
		{
			Slug: "adr-0007-r1", Title: "ADR 7 (r1)", Body: "we indexed quokka tables in sqlite",
			DocType: "decision", EntityID: "adr-0007", RevisionID: "r1",
			Superseded: true, CurrentID: "adr-0007", WordCount: 6,
		},
	}
	if err := wiki.RebuildDB(context.Background(), wikiDir, chunks, nil, nil, nil); err != nil {
		t.Fatalf("RebuildDB: %v", err)
	}

	db, err := wiki.OpenWikiDB(context.Background(), wikiDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = db.Close() }()

	got, err := db.Search(context.Background(), "quokka", 10)
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("got %d hits, want both revisions", len(got))
	}
	var sawCurrent, sawSuperseded bool
	for _, r := range got {
		if r.EntityID != "adr-0007" {
			t.Errorf("entity_id = %q, want adr-0007", r.EntityID)
		}
		if r.Superseded {
			sawSuperseded = true
			if r.CurrentID != "adr-0007" || r.RevisionID != "r1" {
				t.Errorf("superseded hit carries current_id=%q revision_id=%q", r.CurrentID, r.RevisionID)
			}
			continue
		}
		sawCurrent = true
		if r.RevisionID != "" {
			t.Errorf("the current revision carries revision_id=%q, want empty", r.RevisionID)
		}
	}
	if !sawCurrent || !sawSuperseded {
		t.Error("the two revisions did not round-trip with their supersession columns")
	}
}
