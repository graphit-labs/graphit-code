package ast

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// Cancellation had to be asked for, not merely requested: the expensive work is
// inside cgo, which Go cannot preempt. Before this, ctx appeared in pipeline.go
// only in the write phase — Ctrl+C printed "Interrupted — saving progress…" and
// then parsed all 36,823 remaining files to completion.
func TestParseStopsWhenTheCallerHasGivenUp(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")

	// Big enough that a non-cancelling parse would take real time.
	var b strings.Builder
	b.WriteString("<root>")
	for i := 0; i < 20000; i++ {
		b.WriteString("<item attr=\"v\">text</item>")
	}
	b.WriteString("</root>")

	src := filepath.Join(projectDir, "big.xml")
	if err := os.WriteFile(src, []byte(b.String()), 0o644); err != nil {
		t.Fatal(err)
	}

	start := time.Now()
	_, err := NewCompositeParser(projectDir, nil).Parse(src, false, ParseOptions{
		Cancelled: func() bool { return true }, // given up before it began
	})
	elapsed := time.Since(start)

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", err)
	}
	// The point is that it returned at all; a generous bound still fails loudly
	// if the callback is not wired.
	if elapsed > 5*time.Second {
		t.Errorf("took %s to abandon the parse", elapsed)
	}
}

// A cancelled parse must not look like a successful one. An abandoned query
// cursor simply stops yielding matches, so returning its partial entities would
// put a truncated file in the parse cache, which every later run then trusts.
func TestCancelledParseReturnsNoPartialFile(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	src := filepath.Join(projectDir, "a.xml")
	if err := os.WriteFile(src, []byte(`<a><b>x</b></a>`), 0o644); err != nil {
		t.Fatal(err)
	}

	pf, err := NewCompositeParser(projectDir, nil).Parse(src, false, ParseOptions{
		Cancelled: func() bool { return true },
	})
	if err == nil {
		t.Fatal("a cancelled parse reported success")
	}
	if pf != nil {
		t.Errorf("a cancelled parse returned a file with %d entity groups", len(pf.Entities))
	}
}

// Not cancelling must stay exactly as it was — the callback is only installed
// when the caller supplies one, so the common path keeps the cheaper entry
// points.
func TestUncancelledParseIsUnaffected(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	src := filepath.Join(projectDir, "a.xml")
	if err := os.WriteFile(src, []byte(`<a><b>x</b></a>`), 0o644); err != nil {
		t.Fatal(err)
	}

	pf, err := NewCompositeParser(projectDir, nil).Parse(src, false, ParseOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if len(pf.Entities) == 0 {
		t.Error("no entities from an uncancelled parse")
	}
}

// A live context must not be mistaken for a cancelled one.
func TestParseRunsToCompletionWhileTheContextIsAlive(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	src := filepath.Join(projectDir, "a.xml")
	if err := os.WriteFile(src, []byte(`<a><b>x</b></a>`), 0o644); err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	pf, err := NewCompositeParser(projectDir, nil).Parse(src, false, ParseOptions{
		Cancelled: func() bool { return ctx.Err() != nil },
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(pf.Entities) == 0 {
		t.Error("no entities while the context was alive")
	}
}

// "Nothing changed" is only a reason to skip the write when there is something
// to skip to. With the graph deleted and the parse cache current, the pipeline
// returned "N files up to date (no changes detected)" and wrote nothing,
// reporting success over a database that was not there.
//
// This is also the cheap rebuild the CLI has no flag for: delete the database,
// keep the shards, and the write replays them instead of reparsing.
func TestMissingGraphIsRebuiltFromCacheWithoutReparsing(t *testing.T) {
	// A bare temp dir has no grammars: the query files come from the installed
	// runtime, which a test does not have, so the pipeline would parse nothing
	// and prove nothing.
	work := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	if err := os.WriteFile(filepath.Join(work, "a.xml"),
		[]byte(`<a><b>x</b></a>`), 0o644); err != nil {
		t.Fatal(err)
	}

	store := t.TempDir()
	dbPath := filepath.Join(store, "ladybugdb")
	opts := PipelineOptions{CacheDir: filepath.Join(store, "cache")}

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	first, err := RunPipeline(context.Background(), db, work, opts)
	if err != nil {
		t.Fatalf("first pipeline: %v", err)
	}
	_ = db.Close()
	if first.ParsedFiles == 0 {
		t.Fatal("the first run parsed nothing; the fixture is wrong")
	}
	if _, err := os.Stat(dbPath); err != nil {
		t.Fatalf("no database after the first run: %v", err)
	}

	// The graph goes; the parse cache stays.
	if err := os.RemoveAll(dbPath); err != nil {
		t.Fatal(err)
	}

	db2 := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	second, err := RunPipeline(context.Background(), db2, work, opts)
	if err != nil {
		t.Fatalf("second pipeline: %v", err)
	}
	_ = db2.Close()

	if second.ParsedFiles != 0 {
		t.Errorf("reparsed %d file(s); the cache was current and should have been replayed",
			second.ParsedFiles)
	}
	if second.WriteTime == 0 {
		t.Error("no write happened — the run short-circuited over a missing graph")
	}
	if _, err := os.Stat(dbPath); err != nil {
		t.Errorf("the database was not rebuilt: %v", err)
	}
}

// Both write paths build into a `<db>.<hex>` copy and rename it over
// production, so neither opens production read-write — which was the only
// caller of CleanupInterruptedSwap. A copy left behind by a killed process was
// therefore never collected: 369 MB of orphans beside an 81 MB database.
func TestOrphanedSwapCopiesAreCollectedByTheNextWrite(t *testing.T) {
	work := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	if err := os.WriteFile(filepath.Join(work, "a.xml"),
		[]byte(`<a><b>x</b></a>`), 0o644); err != nil {
		t.Fatal(err)
	}

	store := t.TempDir()
	dbPath := filepath.Join(store, "ladybugdb")
	opts := PipelineOptions{CacheDir: filepath.Join(store, "cache")}

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if _, err := RunPipeline(context.Background(), db, work, opts); err != nil {
		t.Fatalf("first pipeline: %v", err)
	}
	_ = db.Close()
	if _, err := os.Stat(dbPath); err != nil {
		t.Fatalf("no database after the first run: %v", err)
	}

	// What a process killed between the copy and the rename leaves behind:
	// the working copy and the engine sidecars belonging to it.
	orphans := []string{
		dbPath + ".7964413",
		dbPath + ".7964413.shadow",
		dbPath + ".7964413.wal.checkpoint",
		dbPath + ".e6ce546",
	}
	for _, o := range orphans {
		if err := os.WriteFile(o, []byte("stale"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	// Any change is enough to reach the write phase.
	if err := os.WriteFile(filepath.Join(work, "b.xml"),
		[]byte(`<c><d>y</d></c>`), 0o644); err != nil {
		t.Fatal(err)
	}

	db2 := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if _, err := RunPipeline(context.Background(), db2, work, opts); err != nil {
		t.Fatalf("second pipeline: %v", err)
	}
	_ = db2.Close()

	for _, o := range orphans {
		if _, err := os.Stat(o); err == nil {
			t.Errorf("orphan survived the write: %s", filepath.Base(o))
		}
	}
	// And the live database is still there — the collector must not take it.
	if _, err := os.Stat(dbPath); err != nil {
		t.Fatalf("the collector removed the live database: %v", err)
	}
}

// The query cursor, not the parser, is where a large data file spends its time,
// so cancellation has to bite there too. This flips mid-flight rather than
// before the call, which is what actually exercises the match loop — an
// always-true callback aborts in the parser and never reaches it.
//
// It is also the case that caught a crash: driving the library's own
// MatchesWithOptions hook here segfaulted inside cgo, because the binding hands
// C a Go-allocated options struct that nothing keeps alive.
func TestCancellationBitesInsideTheMatchLoop(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")

	var b strings.Builder
	b.WriteString("<root>")
	for i := 0; i < 20000; i++ {
		b.WriteString("<item attr=\"v\">text</item>")
	}
	b.WriteString("</root>")
	src := filepath.Join(projectDir, "big.xml")
	if err := os.WriteFile(src, []byte(b.String()), 0o644); err != nil {
		t.Fatal(err)
	}

	// Let the parse finish, then give up once the cursor is iterating.
	var calls int
	pf, err := NewCompositeParser(projectDir, nil).Parse(src, false, ParseOptions{
		Cancelled: func() bool {
			calls++
			return calls > 50
		},
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", err)
	}
	if pf != nil {
		t.Error("a cancelled parse returned a partial file")
	}
	if calls <= 50 {
		t.Errorf("the callback was polled %d times; the match loop never ran", calls)
	}
}
