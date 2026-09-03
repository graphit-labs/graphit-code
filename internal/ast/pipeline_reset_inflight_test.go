package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func TestParsePoolResetsAntlrCachesWithParsesInFlight(t *testing.T) {
	const files = 24

	proj := stageAntlr(t, "plsql.yaml")

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
		src := fmt.Sprintf(
			"CREATE OR REPLACE FUNCTION f_%d(p_a NUMBER) RETURN NUMBER IS\nBEGIN\n  IF p_a > %d THEN RETURN %d; END IF;\n  RETURN 0;\nEND;\n",
			i, i, i*7)
		if err := os.WriteFile(filepath.Join(proj, fmt.Sprintf("f_%d.sql", i)), []byte(src), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}

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
