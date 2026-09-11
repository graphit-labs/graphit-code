//go:build lancedb

package memory

import (
	"context"
	"strings"
	"testing"
)

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

	results, err := svc.SearchMemories(context.Background(), "quokka marker", 3, SearchOptions{})
	if err != nil {
		t.Fatal(err)
	}
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
	plain := []ChainResult{{Path: "a", Title: "A", DocType: "fact", Score: 1}}
	if got := FormatChainResultsTOON(plain, false); strings.Contains(got, "superseded") {
		t.Errorf("the header carries chain columns for an answer with no superseded hit:\n%s", got)
	}

	mixed := []ChainResult{plain[0], {
		Path: "b/rev", Title: "B", DocType: "fact", Score: 0.5,
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

func TestChainResolvesDirectlyFromTheAuthoritativeTable(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("Column-resolved", "the marker word wombat and an old detail", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Column-resolved", "the marker word wombat and a new detail"); err != nil {
		t.Fatal(err)
	}

	results, err := svc.SearchMemories(context.Background(), "wombat", 10, SearchOptions{})
	if err != nil {
		t.Fatal(err)
	}
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
