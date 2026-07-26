package ast

import (
	"fmt"
	"path/filepath"
	"testing"
	"time"
)

// bigSyntheticCache builds a parse cache of roughly `total` entities with Oracle-shaped
// names over a small vocabulary, so shared tokens and trigrams are genuinely common.
func bigSyntheticCache(t *testing.T, dir string, total int) *ShardCache {
	return syntheticCache(t, dir, total, 200)
}

// syntheticCache spreads `total` entities over files of `perFile` each. The file count
// matters independently of the entity count: an FTS inconsistency on delete showed up only
// once the file table held a few thousand nodes.
func syntheticCache(t *testing.T, dir string, total, perFile int) *ShardCache {
	t.Helper()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	verbs := []string{"EXTRAIR", "ATUALIZA", "VALIDA", "CONF", "CFG", "CARGA", "PROC", "GRAVA"}
	nouns := []string{"DOC_LOTE", "ENTRG", "CONTA", "SCHEMA", "CONFIG", "CHECKSUM", "PEDIDO", "CLIENTE"}
	for i := 0; i < total/perFile; i++ {
		path := fmt.Sprintf("pkg/pkg_%05d.sql", i)
		ents := make([]cachedEntity, 0, perFile)
		for j := 0; j < perFile; j++ {
			n := i*perFile + j
			ents = append(ents, cachedEntity{
				Label: "Procedure",
				UID:   fmt.Sprintf("u%d", n),
				Name: fmt.Sprintf("%s_%s_%04d",
					verbs[n%len(verbs)], nouns[(n/len(verbs))%len(nouns)], n%9973),
				Path: path, Line: j + 1, EndLine: j + 1,
			})
		}
		if err := pc.Store(path, "h"+path, &parseCacheEntry{
			RelPath: path, Language: "sql", Source: "-- " + path + "\n", Entities: ents,
		}); err != nil {
			t.Fatalf("store: %v", err)
		}
	}
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	return pc
}

// TestSearchIndexScaleCost measures the cost the design's one compromise imposes.
//
// Rows written into a table whose FTS index already exists are only intermittently
// indexed (see createIndexes), so UpdateIncremental drops and recreates the FTS indexes
// after writing. That is correct but not obviously affordable: recreating seven indexes
// over the whole corpus to reflect one edited file is O(corpus) work for O(1) change,
// while the incremental path it replaces runs in ~330 ms end to end.
//
// If that cost is prohibitive, the design needs revisiting before SQLite is removed —
// which is exactly why this is measured and reported rather than assumed away.
func TestSearchIndexScaleCost(t *testing.T) {
	if testing.Short() {
		t.Skip("builds a large index")
	}

	const total = 200_000
	dir := t.TempDir()
	cache := bigSyntheticCache(t, filepath.Join(dir, "cache"), total)

	lb, err := OpenSearchIndex(filepath.Join(dir, "search"))
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { _ = lb.Close() })

	t0 := time.Now()
	if err := lb.RebuildFromCache(cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	fullRebuild := time.Since(t0)
	t.Logf("full rebuild of %d entities: %s", total, fullRebuild.Round(time.Millisecond))

	// Query latency, comparable to TestTrigramBagSearchLatency on the SQLite index.
	queries := []string{"config", "conf", "checksum", "ENTRG", "EXTRAIR_DOC", "validaSchema"}
	for _, q := range queries {
		_, _ = lb.Search(q, 20)
	}
	var worstQuery time.Duration
	var worstName string
	for _, q := range queries {
		start := time.Now()
		res, err := lb.Search(q, 20)
		el := time.Since(start)
		if err != nil {
			t.Errorf("search %q: %v", q, err)
			continue
		}
		if len(res) == 0 {
			t.Errorf("search %q returned nothing on a %d-entity index", q, total)
		}
		if el > worstQuery {
			worstQuery, worstName = el, q
		}
		t.Logf("  search %-14s %8s  (%d results)", q, el.Round(time.Microsecond), len(res))
	}
	t.Logf("slowest query: %q at %s", worstName, worstQuery.Round(time.Millisecond))

	// One edited file: the operation the daemon performs on every keystroke-triggered
	// reindex.
	changed := "pkg/pkg_00007.sql"
	entry := cache.GetEntry(changed)
	if entry == nil {
		t.Fatalf("missing entry for %s", changed)
	}
	entry.Entities[0].Name = "EXTRAIR_DOC_LOTE_RENAMED"
	if err := cache.Store(changed, "h2", entry); err != nil {
		t.Fatalf("store changed: %v", err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	t1 := time.Now()
	if err := lb.UpdateIncremental(cache, []string{changed}, nil, nil); err != nil {
		t.Fatalf("incremental update: %v", err)
	}
	incremental := time.Since(t1)
	t.Logf("incremental update of ONE file on a %d-entity index: %s",
		total, incremental.Round(time.Millisecond))
	t.Logf("  ratio to full rebuild: %.1f%%", 100*incremental.Seconds()/fullRebuild.Seconds())

	res, err := lb.Search("renamed", 10)
	if err != nil {
		t.Fatalf("search after update: %v", err)
	}
	found := false
	for _, r := range res {
		if r.Name == "EXTRAIR_DOC_LOTE_RENAMED" {
			found = true
		}
	}
	if !found {
		t.Error("the renamed entity is not searchable after the incremental update")
	}

	// The incremental path exists so an edit does not cost a rebuild. A ceiling rather
	// than a target: the logged figure is what to judge, and it is compared against the
	// ~330 ms the current pipeline achieves end to end.
	if incremental > 10*time.Second {
		t.Errorf("incremental update took %s on %d entities — recreating the FTS indexes per edit "+
			"is not viable at corpus scale and the design needs revisiting",
			incremental.Round(time.Millisecond), total)
	}
}

// TestSearchIndexIncrementalRepeatedAtScale reproduces the FTS inconsistency that a small
// corpus hides.
//
// TestSearchIndexIncrementalRepeated runs eight consecutive updates over twenty entities and
// passes. The same loop over a few thousand FILES fails on the fourth update with
//
//	FTS index 'sf_source' is inconsistent: document for node offset 3002 is missing during
//	delete. Drop and recreate the FTS index.
//
// after which UpdateIncremental returns early and every later update is silently skipped —
// which the end-to-end benchmark misread as the incremental path getting 40x faster.
// The file count is what matters, not the entity count, so this uses many small files.
func TestSearchIndexIncrementalRepeatedAtScale(t *testing.T) {
	if testing.Short() {
		t.Skip("builds a few thousand files")
	}

	const files = 3200
	const perFile = 2

	dir := t.TempDir()
	cache := syntheticCache(t, filepath.Join(dir, "cache"), files*perFile, perFile)

	lb, err := OpenSearchIndex(filepath.Join(dir, "search"))
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { _ = lb.Close() })
	if err := lb.RebuildFromCache(cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	const target = "pkg/pkg_00007.sql"
	for round := 1; round <= 6; round++ {
		name := fmt.Sprintf("MARCADOR_RODADA_%d", round)
		if err := cache.Store(target, fmt.Sprintf("h%d", round), &parseCacheEntry{
			RelPath: target, Language: "sql", Source: "-- " + target + "\n",
			Entities: []cachedEntity{{
				Label: "Procedure", UID: "u-target", Name: name,
				Path: target, Line: 1, EndLine: 1,
			}},
		}); err != nil {
			t.Fatalf("round %d store: %v", round, err)
		}
		if err := cache.FlushDirty(); err != nil {
			t.Fatalf("round %d flush: %v", round, err)
		}

		start := time.Now()
		if err := lb.UpdateIncremental(cache, []string{target}, nil, nil); err != nil {
			t.Fatalf("round %d update failed on a %d-file index: %v", round, files, err)
		}
		elapsed := time.Since(start)

		res, err := lb.Search(name, 20)
		if err != nil {
			t.Fatalf("round %d search: %v", round, err)
		}
		found := false
		for _, r := range res {
			if r.Name == name {
				found = true
			}
		}
		t.Logf("round %d: %s, searchable=%v", round, elapsed.Round(time.Millisecond), found)
		if !found {
			t.Errorf("round %d: %q is not searchable after its update — the index went stale",
				round, name)
		}
	}
}
