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

func twinFile(t *testing.T, w *ScopeStore, name, id, title, body string) {
	t.Helper()
	content := "---\nid: " + id + "\ntitle: " + title +
		"\nscope: project\nscope_id: p\ntype: fact\nimportant: true\nrevision: 1\ntags: [memory, project, fact]\n---\n\n# " +
		title + "\n\n" + body + "\n"
	if err := w.WriteFile(name, []byte(content)); err != nil {
		t.Fatal(err)
	}
}

// A twin whose body is already in the live memory carries nothing, so it is removed.
func TestRepairRemovesATwinWhoseBodyAlreadyExists(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Shared", "identical body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	twinFile(t, w, id+"_important_.md", id+"_important_", "Shared", "identical body")

	report, err := svc.RepairForkedMemories()
	if err != nil {
		t.Fatalf("RepairForkedMemories: %v", err)
	}
	if len(report.Removed) != 1 {
		t.Errorf("removed %v, want exactly the twin", report.Removed)
	}
	if len(report.Archived) != 0 {
		t.Errorf("archived %v, want nothing — the body was not unique", report.Archived)
	}
	if _, err := w.ReadFile(id + "_important_.md"); !os.IsNotExist(err) {
		t.Errorf("the twin survived: %v", err)
	}
	if _, err := w.ReadFile(MemoryFileName(id)); err != nil {
		t.Errorf("the live memory was removed instead of the twin: %v", err)
	}
}

// A twin whose body exists nowhere is knowledge, so it becomes a superseded revision of the chain
// rather than a deletion.
func TestRepairArchivesADivergentTwin(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Diverged", "the live body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	twinFile(t, w, id+"_important_.md", id+"_important_", "Diverged elsewhere", "a rewrite that exists nowhere else")

	report, err := svc.RepairForkedMemories()
	if err != nil {
		t.Fatalf("RepairForkedMemories: %v", err)
	}
	if len(report.Archived) != 1 {
		t.Fatalf("archived %v, want exactly one revision", report.Archived)
	}
	if _, err := w.ReadFile(id + "_important_.md"); !os.IsNotExist(err) {
		t.Errorf("the twin survived: %v", err)
	}

	archived, err := w.ReadFile(report.Archived[0])
	if err != nil {
		t.Fatalf("the archived revision cannot be read: %v", err)
	}
	if !strings.Contains(string(archived), "a rewrite that exists nowhere else") {
		t.Error("the archive does not hold the twin's content")
	}

	fm := ParseMemoryFrontmatter(string(archived))
	if fm.ID != id {
		t.Errorf("archived id = %q, want the chain id %q — the corrupted id must not survive", fm.ID, id)
	}
	if fm.Next != MemoryFileName(id) {
		t.Errorf("archived next = %q, want %q", fm.Next, MemoryFileName(id))
	}
	if !fm.IsArchivedRevision() {
		t.Error("the archive does not report itself as a superseded revision")
	}
}

// With no live memory for the chain the twin is the only copy, so deleting it would be the loss
// this repair exists to prevent.
func TestRepairPromotesAnOrphanTwin(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)
	if err := svc.EnsureInitialised(); err != nil {
		t.Fatal(err)
	}

	id := "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	twinFile(t, w, id+"_important_.md", id+"_important_", "Orphan", "the only copy of this knowledge")

	report, err := svc.RepairForkedMemories()
	if err != nil {
		t.Fatalf("RepairForkedMemories: %v", err)
	}
	if len(report.Promoted) != 1 {
		t.Fatalf("promoted %v, want exactly one", report.Promoted)
	}

	live, err := w.ReadFile(MemoryFileName(id))
	if err != nil {
		t.Fatalf("the orphan was not promoted to the live memory: %v", err)
	}
	fm := ParseMemoryFrontmatter(string(live))
	if fm.ID != id {
		t.Errorf("promoted id = %q, want %q", fm.ID, id)
	}
	if fm.IsArchivedRevision() {
		t.Error("the promoted memory still reports itself as a superseded revision")
	}
	if !strings.Contains(string(live), "the only copy of this knowledge") {
		t.Error("the promoted memory lost its content")
	}
	if _, err := w.ReadFile(id + "_important_.md"); !os.IsNotExist(err) {
		t.Errorf("the twin survived promotion: %v", err)
	}
}

// A twin's own history directory belongs to the chain it was forked from.
func TestRepairFoldsAForkedHistoryDirectory(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("With forked history", "live body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}

	forked := HistoryDirFor(id+"_important_") + "/0001.md"
	twinFile(t, w, forked, id+"_important_", "Old forked revision", "content only the forked history has")

	report, err := svc.RepairForkedMemories()
	if err != nil {
		t.Fatalf("RepairForkedMemories: %v", err)
	}
	if len(report.Archived) != 1 {
		t.Fatalf("archived %v, want the forked revision folded into the chain", report.Archived)
	}
	if _, err := w.ReadFile(forked); !os.IsNotExist(err) {
		t.Errorf("the forked revision survived in its own directory: %v", err)
	}

	archived, err := w.ReadFile(report.Archived[0])
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(archived), "content only the forked history has") {
		t.Error("folding the forked history lost its content")
	}
	if got := ParseMemoryFrontmatter(string(archived)).ID; got != id {
		t.Errorf("folded revision id = %q, want the chain id %q", got, id)
	}
}

// It runs on every index, so doing nothing has to be the cheap and silent case.
func TestRepairIsIdempotent(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Clean", "body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	twinFile(t, w, id+"_important_.md", id+"_important_", "Clean", "a divergent body")

	first, err := svc.RepairForkedMemories()
	if err != nil {
		t.Fatal(err)
	}
	if !first.Changed() {
		t.Fatal("the first pass reported no change, but there was a twin")
	}

	second, err := svc.RepairForkedMemories()
	if err != nil {
		t.Fatal(err)
	}
	if second.Changed() {
		t.Errorf("the second pass changed something: %s", second)
	}
}

// The two guards that stop the fork recurring, tested at the compile boundary: a forked name is
// not a memory, and one declared id compiles to one page.
func TestForkedFilesDoNotCompileIntoTheWiki(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Only me", "body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	twinFile(t, w, id+"_important_.md", id+"_important_", "Only me", "twin body")

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	if _, err := GenerateMemoryWiki(context.Background(), w.Dir(), wikiDir); err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}

	live, superseded, ids := indexedMemoryPages(t, wikiDir)
	if live != 1 || superseded != 0 {
		t.Errorf("indexed %d live and %d superseded rows, want 1 and 0 — the twin reached the index", live, superseded)
	}
	for _, got := range ids {
		if got != id {
			t.Errorf("indexed entity_id %q, want the chain id %q", got, id)
		}
	}
}

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

func TestChainIDOf(t *testing.T) {
	const id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	tests := []struct {
		content string
		name    string
		want    string
	}{
		{"", id + ".md", id},
		{"", id + "_important_.md", id},
		{"---\nid: " + id + "_important_\n---\n", "whatever.md", id},
		{"---\nid: " + id + "\n---\n", id + "_important_.md", id},
		{"", "not-an-id.md", ""},
	}
	for _, tc := range tests {
		if got := chainIDOf(tc.content, tc.name); got != tc.want {
			t.Errorf("chainIDOf(%q, %q) = %q, want %q", tc.content, tc.name, got, tc.want)
		}
	}
}

// A search result budget must count memories, not index rows: collapsing a chain after ranking
// would otherwise silently shrink the answer.
func TestTopKCountsDistinctMemories(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

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
	if _, err := GenerateMemoryWiki(context.Background(), w.Dir(), wikiDir); err != nil {
		t.Fatal(err)
	}

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
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Column-resolved", "the marker word wombat and an old detail", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Column-resolved", "the marker word wombat and a new detail"); err != nil {
		t.Fatal(err)
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	if _, err := GenerateMemoryWiki(context.Background(), w.Dir(), wikiDir); err != nil {
		t.Fatal(err)
	}

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
