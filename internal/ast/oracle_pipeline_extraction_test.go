package ast

import (
	"context"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestOraclePipelineExtraction closes the gap between two measurements that cannot both be
// right.
//
// Parsing real Oracle files directly yields entities — 73 across six files, all from
// antlr-plsql, with tree-sitter-sql contributing zero (TestOracleCorpusExtraction). Running
// the same corpus through the pipeline reports "empty=799 errors=0": every file parsed, none
// produced an entity. The parse timing says the same thing twice: 452 ms for 799 files in
// the pipeline against roughly 70 ms for a single file in isolation, so the pipeline is not
// running the ANTLR parse at all.
//
// Both use NewCompositeParser with the same options, so the difference is environmental.
// This runs the pipeline over a handful of real files and counts what landed in the cache,
// which is the same code path with everything else held equal.
func TestOraclePipelineExtraction(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}

	var samples []string
	seenDir := map[string]bool{}
	_ = filepath.WalkDir(src, func(p string, d fs.DirEntry, e error) error {
		if e != nil || d.IsDir() || !strings.HasSuffix(strings.ToLower(p), ".sql") {
			return nil
		}
		dir := filepath.Dir(p)
		if seenDir[dir] {
			return nil
		}
		seenDir[dir] = true
		samples = append(samples, p)
		if len(samples) >= 4 {
			return filepath.SkipAll
		}
		return nil
	})
	if len(samples) == 0 {
		t.Skip("no .sql files under GRAPHIT_E2E_SQL_DIR")
	}

	tmp := t.TempDir()
	work := filepath.Join(tmp, "corpus")
	if err := os.MkdirAll(work, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, p := range samples {
		b, err := os.ReadFile(p)
		if err != nil {
			t.Fatalf("read %s: %v", p, err)
		}
		if err := os.WriteFile(filepath.Join(work, filepath.Base(p)), b, 0o644); err != nil {
			t.Fatal(err)
		}
	}

	direct := 0
	comp := NewCompositeParser(work, nil)
	for _, p := range samples {
		dst := filepath.Join(work, filepath.Base(p))
		pf, err := comp.Parse(dst, false, ParseOptions{IndexSource: true})
		if err != nil {
			t.Logf("direct parse %s: %v", filepath.Base(p), err)
			continue
		}
		if pf != nil {
			t.Logf("direct  %-40s engine=%-12s entities=%d", filepath.Base(p), pf.Parser, pf.EntityCount())
			direct += pf.EntityCount()
		}
	}

	dbPath := filepath.Join(tmp, "ladybugdb")
	cacheDir := filepath.Join(tmp, "cache")
	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = db.Close() }()

	res, err := RunPipeline(context.Background(), db, work, PipelineOptions{
		CacheDir:    cacheDir,
		IndexSource: true,
	})
	if err != nil {
		t.Fatalf("pipeline: %v", err)
	}
	t.Logf("pipeline: files=%d parsed=%d empty=%d errors=%d",
		res.TotalFiles, res.ParsedFiles, res.EmptyCount, res.ErrorCount)

	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("open cache: %v", err)
	}
	viaPipeline := 0
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		if entry == nil {
			return true
		}
		t.Logf("cached  %-40s entities=%d", relPath, len(entry.Entities))
		viaPipeline += len(entry.Entities)
		return true
	})

	t.Logf("entities: direct parse=%d, through the pipeline=%d", direct, viaPipeline)

	if direct == 0 {
		t.Skip("the direct parse also produced nothing; TestOracleCorpusExtraction covers that case")
	}
	if viaPipeline == 0 {
		t.Errorf("the pipeline stored NO entities for files that yield %d when parsed directly — "+
			"the graph and the search index are being built from nothing", direct)
	}
}
