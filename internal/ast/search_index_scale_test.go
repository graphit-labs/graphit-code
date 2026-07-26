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
	t.Helper()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	verbs := []string{"EXTRAIR", "ATUALIZA", "VALIDA", "CONF", "CFG", "CARGA", "PROC", "GRAVA"}
	nouns := []string{"DOC_LOTE", "ENTRG", "CONTA", "SCHEMA", "CONFIG", "CHECKSUM", "PEDIDO", "CLIENTE"}
	const perFile = 200
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
