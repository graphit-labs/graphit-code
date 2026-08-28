package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// The parse pool used to reset the ANTLR caches only at a chunk barrier, where
// wg.Wait() guaranteed no parse was in flight. That barrier was what made every
// chunk cost its slowest file, so it is gone: the reset is now issued from a
// worker while its peers are still parsing, and safety comes from antlrcommon's
// RWMutex instead — drivers take the read lock for the whole of a parse,
// ResetAllCaches takes the write lock.
//
// This exercises that arrangement through the real pipeline, with real ANTLR
// PL/SQL parses in flight: interval 1 and a heap limit of 0 make the pressure
// check fire after EVERY file, so a reset overlaps its peers' parses as often as
// possible.
//
// It fails three ways, which is the point:
//   - deadlock (caught by the test timeout) if a reset is ever issued from
//     inside a parse, i.e. by a goroutine already holding the read lock;
//   - a data race under -race if the lock stops covering parser construction;
//   - a short file count if the continuous pool drops work — the producer, the
//     workers and the shared reset counter are one interacting piece now, where
//     each chunk used to get a fresh producer and pool.
func TestParsePoolResetsAntlrCachesWithParsesInFlight(t *testing.T) {
	const files = 24

	proj := stageAntlr(t, "plsql.yaml")

	// The extension tables are built from the runtime and user query dirs, so a
	// project-local grammar is discoverable but not selectable through them:
	// register .sql here so discovery picks the fixtures up, and pin the grammar
	// with an override so the parse goes straight to ANTLR with no tree-sitter
	// attempt first.
	cfg := &antlrLangConfig{
		Language:   "plsql",
		Grammar:    "antlr-plsql",
		Extensions: []string{".sql"},
	}
	extTablesMu.Lock()
	restoreExt, hadExt := antlrExtMap[".sql"]
	restoreGrammar, hadGrammar := antlrGrammarMap["antlr-plsql"]
	antlrExtMap[".sql"] = []*antlrLangConfig{cfg}
	if antlrGrammarMap == nil {
		antlrGrammarMap = map[string]*antlrLangConfig{}
	}
	antlrGrammarMap["antlr-plsql"] = cfg
	extTablesMu.Unlock()
	t.Cleanup(func() {
		extTablesMu.Lock()
		defer extTablesMu.Unlock()
		if hadExt {
			antlrExtMap[".sql"] = restoreExt
		} else {
			delete(antlrExtMap, ".sql")
		}
		if hadGrammar {
			antlrGrammarMap["antlr-plsql"] = restoreGrammar
		} else {
			delete(antlrGrammarMap, "antlr-plsql")
		}
	})

	for i := 0; i < files; i++ {
		// Distinct bodies on purpose: identical input would be answered from the
		// warm DFA, and it is the newly seen patterns that make a reset cost
		// anything to overlap with.
		src := fmt.Sprintf(
			"CREATE OR REPLACE FUNCTION f_%d(p_a NUMBER) RETURN NUMBER IS\nBEGIN\n  IF p_a > %d THEN RETURN %d; END IF;\n  RETURN 0;\nEND;\n",
			i, i, i*7)
		if err := os.WriteFile(filepath.Join(proj, fmt.Sprintf("f_%d.sql", i)), []byte(src), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}

	// Reset after every file, and always consider the heap under pressure. Not
	// t.Parallel-safe, which is why this test does not declare it.
	restoreInterval, restoreLimit := antlrCacheCheckInterval, antlrCacheHeapLimit
	antlrCacheCheckInterval, antlrCacheHeapLimit = 1, 0
	t.Cleanup(func() {
		antlrCacheCheckInterval, antlrCacheHeapLimit = restoreInterval, restoreLimit
	})

	tmp := t.TempDir()
	dbPath := filepath.Join(tmp, "ladybugdb")
	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	res, err := RunPipeline(context.Background(), db, proj, PipelineOptions{
		Workers:          4,
		CacheDir:         filepath.Join(tmp, "cache"),
		GrammarOverrides: map[string]string{".sql": "antlr-plsql"},
	})
	if err != nil {
		_ = db.Close()
		t.Fatalf("pipeline: %v", err)
	}
	_ = db.Close()

	if res.ErrorCount > 0 {
		t.Errorf("parse errors with resets in flight: %d %v", res.ErrorCount, res.ErrorFiles)
	}
	if res.ParsedFiles != files {
		t.Errorf("parsed %d files, want %d — the continuous pool dropped work", res.ParsedFiles, files)
	}

	// Reopened, because the pipeline swaps the database file underneath.
	graph := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = graph.Close() }()

	got, err := graph.Query(context.Background(),
		"MATCH (f:File) WHERE f.path ENDS WITH '.sql' RETURN count(f) AS n", nil)
	if err != nil {
		t.Fatalf("count files: %v", err)
	}
	if len(got.Records) != 1 {
		t.Fatalf("expected one row, got %v", got.Records)
	}
	if n := fmt.Sprint(got.Records[0]["n"]); n != fmt.Sprint(files) {
		t.Errorf("graph holds %s .sql files, want %d — a reset lost a parse", n, files)
	}
}
