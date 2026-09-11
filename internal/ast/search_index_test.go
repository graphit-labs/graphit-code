//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"
	"testing"
)

func buildSearchIndex(t *testing.T, dir string, cache *ShardCache,
	embLookup func(relPath, uid string) []float32) *SearchIndex {
	t.Helper()
	si, err := OpenSearchIndex(context.Background(), filepath.Join(dir, "ladybugdb"))
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	t.Cleanup(func() { _ = si.Close() })
	if err := si.RebuildFromCache(context.Background(), cache, nil); err != nil {
		t.Fatalf("rebuild search index: %v", err)
	}
	applyVectors(t, si, cache, embLookup)
	return si
}

func applyVectors(t *testing.T, si *SearchIndex, cache *ShardCache,
	embLookup func(relPath, uid string) []float32) {
	t.Helper()
	if embLookup == nil {
		return
	}
	var ents []cachedEntity
	var vecs [][]float32
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		for _, e := range entry.Entities {
			if v := embLookup(relPath, e.UID); v != nil {
				ents = append(ents, e)
				vecs = append(vecs, v)
			}
		}
		return true
	})
	if len(ents) == 0 {
		return
	}
	if err := si.StoreEntityVectors(context.Background(), ents, vecs); err != nil {
		t.Fatalf("store vectors: %v", err)
	}
	if err := si.FinalizeVectors(context.Background()); err != nil {
		t.Fatalf("finalize vectors: %v", err)
	}
}

func indexSearchNames(t *testing.T, si *SearchIndex, query string, topK int) []string {
	t.Helper()
	res, err := si.Search(context.Background(), query, topK)
	if err != nil {
		t.Logf("  search %q failed: %v", query, err)
		return nil
	}
	var out []string
	seen := map[string]bool{}
	for _, r := range res {
		n := r.Name
		if i := strings.IndexByte(n, ' '); i > 0 {
			n = n[:i]
		}
		if seen[n] {
			continue
		}
		seen[n] = true
		out = append(out, n)
	}
	return out
}

func TestSearchIndexQualityFloor(t *testing.T) {
	dir := t.TempDir()
	corpus := prefixCorpus()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)
	si := buildSearchIndex(t, dir, cache, nil)

	cases := []struct{ query, wantTop string }{
		{"parseConfig", "parseConfig"},
		{"checksum", "computeChecksum"},
		{"retry backoff", "retryPolicy"},
		{"parse sql", "parseSQL"},
		{"conf", "CONF_MGR"},
		{"compu", "computeChecksum"},
		{"retr", "retryPolicy"},
		{"connect", "connectDatabase"},
		{"audit", "TRG_AUDITORIA_CLIENTE"},
		{"extrair", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{"cf", "CFG_LOAD"},
	}
	const baselineTop1 = 11

	recall := []struct{ query, wantAny string }{
		{"configuration", "parseConfig"},
		{"schema", "validateSchema"},
		{"config", "configLoader"},
		{"valid", "validateSchema"},
		{"valida", "PKG_VALIDACAO_PAGAMENTO"},
	}

	t.Logf("%-16s | %-30s | %s", "query", "expected top-1", "got")
	t.Logf("%s", strings.Repeat("-", 84))

	var hits, empty int
	for _, c := range cases {
		res, err := si.Search(context.Background(), c.query, 5)
		if err != nil {
			t.Errorf("search %q: %v", c.query, err)
			continue
		}
		names := entityNames(res, 5)
		top := ""
		if len(names) > 0 {
			top = names[0]
		} else {
			empty++
		}
		if top == c.wantTop {
			hits++
		}
		mark := top
		if mark == "" {
			mark = "(vazio)"
		} else if top == c.wantTop {
			mark += " OK"
		}
		t.Logf("%-16s | %-30s | %s", c.query, c.wantTop, mark)
	}

	const recallAt5 = 5
	var reached int
	for _, c := range recall {
		res, err := si.Search(context.Background(), c.query, recallAt5)
		if err != nil {
			t.Errorf("search %q: %v", c.query, err)
			continue
		}
		names := entityNames(res, recallAt5)
		found := false
		for _, n := range names {
			if n == c.wantAny {
				found = true
			}
		}
		if found {
			reached++
		}
		t.Logf("%-16s | %-30s | recall@%d: %v %v", c.query, c.wantAny, recallAt5, found, names)
		if !found {
			t.Errorf("QUALITY FLOOR: %q does not reach %q anywhere in the top %d: %v",
				c.query, c.wantAny, recallAt5, names)
		}
	}

	t.Logf("%s", strings.Repeat("-", 84))
	t.Logf("strict top-1: %d/%d   recall@%d: %d/%d   empty: %d",
		hits, len(cases), recallAt5, reached, len(recall), empty)

	if hits < baselineTop1 {
		t.Errorf("QUALITY FLOOR: the expected entity ranked first %d/%d times, below the measured "+
			"%d/%d on the same corpus and probes", hits, len(cases), baselineTop1, len(cases))
	}
	if empty > 0 {
		t.Errorf("QUALITY FLOOR: %d queries returned nothing; the measured baseline had none", empty)
	}
}

// TestSearchIndexRebuildIsIdempotent guards the atomic swap: rebuilding must
// replace the contents, not accumulate them, and must leave the index queryable.
func TestSearchIndexRebuildIsIdempotent(t *testing.T) {
	dir := t.TempDir()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), gateCorpus())
	lb := buildSearchIndex(t, dir, cache, nil)

	first, err := lb.Search(context.Background(), "config", 20)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(first) == 0 {
		t.Fatal("first rebuild produced an index that matches nothing")
	}

	for i := 0; i < 3; i++ {
		if err := lb.RebuildFromCache(context.Background(), cache, nil); err != nil {
			t.Fatalf("rebuild %d: %v", i+2, err)
		}
		got, err := lb.Search(context.Background(), "config", 20)
		if err != nil {
			t.Fatalf("search after rebuild %d: %v", i+2, err)
		}
		if len(got) != len(first) {
			t.Errorf("rebuild %d changed the result count: %d -> %d (duplicates or losses)",
				i+2, len(first), len(got))
		}
		if fingerprint(got) != fingerprint(first) {
			t.Errorf("rebuild %d changed results:\n first: %v\n got:   %v",
				i+2, fingerprint(first), fingerprint(got))
		}
	}
}

func fingerprint(res []SearchResult) string {
	parts := make([]string, 0, len(res))
	for _, r := range res {
		parts = append(parts, fmt.Sprintf("%s:%s:%s:%d", r.Type, r.Path, r.Name, r.Line))
	}
	return strings.Join(parts, "|")
}

// TestSearchIndexIncremental exercises the incremental update path: stale rows go, new rows
// arrive, and repeating the same update does not duplicate anything.
func TestSearchIndexIncremental(t *testing.T) {
	dir := t.TempDir()
	corpus := gateCorpus()
	cachePath := filepath.Join(dir, "cache")
	cache := cacheFromCorpus(t, cachePath, corpus)
	lb := buildSearchIndex(t, dir, cache, nil)

	find := func(query, want string) bool {
		res, err := lb.Search(context.Background(), query, 20)
		if err != nil {
			t.Fatalf("search %q: %v", query, err)
		}
		for _, n := range entityNames(res, 0) {
			if n == want {
				return true
			}
		}
		return false
	}

	if !find("checksum", "computeChecksum") {
		t.Fatal("baseline: computeChecksum not found before the incremental update")
	}

	if err := cache.Store("hash.go", "h2", &parseCacheEntry{
		RelPath: "hash.go", Language: "go", Source: "// hash.go v2\n",
		Entities: []cachedEntity{{
			Label: "Function", UID: "u9b", Name: "renamedDigest",
			Path: "hash.go", Line: 1, EndLine: 1, Docstring: "Computes a digest.",
		}},
	}); err != nil {
		t.Fatalf("store changed file: %v", err)
	}
	cache.Remove("db.go")
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	if err := lb.UpdateIncremental(context.Background(), cache, []string{"hash.go"}, []string{"db.go"}, nil); err != nil {
		t.Fatalf("incremental update: %v", err)
	}

	if find("checksum", "computeChecksum") {
		t.Error("computeChecksum survived after its file was rewritten — stale rows are not removed")
	}
	if !find("digest", "renamedDigest") {
		t.Error("renamedDigest not found after the incremental update — new rows are not indexed")
	}
	if find("database", "connectDatabase") {
		t.Error("connectDatabase survived after db.go was deleted")
	}

	if err := lb.UpdateIncremental(context.Background(), cache, []string{"hash.go"}, []string{"db.go"}, nil); err != nil {
		t.Fatalf("repeated incremental update: %v", err)
	}
	res, err := lb.Search(context.Background(), "digest", 20)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	count := 0
	for _, n := range entityNames(res, 0) {
		if n == "renamedDigest" {
			count++
		}
	}
	if count != 1 {
		t.Errorf("renamedDigest appears %d times after a repeated update, want 1 (delete-then-insert is not idempotent)", count)
	}
}

// TestSearchIndexIncrementalRepeated runs consecutive incremental updates, which is what a
// daemon does all day and what two updates do not exercise.
//
// Two was not enough: with the FTS indexes recreated AFTER the writes, the fourth update's
// DELETE hit an index that no longer knew the node it was deleting —
//
//	FTS index 'sf_source' is inconsistent: document for node offset 3002 is missing during
//	delete. Drop and recreate the FTS index.
//
// — so UpdateIncremental returned early and the update was silently skipped. The end-to-end
// benchmark read that as a speed-up (48 ms instead of 1.9 s) because a failing no-op is fast,
// and the error was being discarded at the call site. Both are fixed; this is the test that
// would have caught it.
func TestSearchIndexIncrementalRepeated(t *testing.T) {
	dir := t.TempDir()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), prefixCorpus())
	lb := buildSearchIndex(t, dir, cache, nil)

	const rounds = 8
	for round := 1; round <= rounds; round++ {
		name := fmt.Sprintf("renamedRound%d", round)
		if err := cache.Store("hash.go", fmt.Sprintf("h%d", round), &parseCacheEntry{
			RelPath: "hash.go", Language: "go", Source: "// hash.go\n",
			Entities: []cachedEntity{{
				Label: "Function", UID: "u9r", Name: name,
				Path: "hash.go", Line: 1, EndLine: 1,
				Docstring: "Round " + fmt.Sprint(round) + " marker.",
			}},
		}); err != nil {
			t.Fatalf("round %d store: %v", round, err)
		}
		if err := cache.FlushDirty(); err != nil {
			t.Fatalf("round %d flush: %v", round, err)
		}

		if err := lb.UpdateIncremental(context.Background(), cache, []string{"hash.go"}, nil, nil); err != nil {
			t.Fatalf("round %d update: %v", round, err)
		}

		res, err := lb.Search(context.Background(), name, 20)
		if err != nil {
			t.Fatalf("round %d search: %v", round, err)
		}
		found := false
		for _, n := range entityNames(res, 0) {
			if n == name {
				found = true
			}
		}
		if !found {
			t.Errorf("round %d: %q is not searchable after its incremental update", round, name)
		}
		if round > 1 {
			prev := fmt.Sprintf("renamedRound%d", round-1)
			res, err := lb.Search(context.Background(), prev, 20)
			if err != nil {
				t.Fatalf("round %d stale search: %v", round, err)
			}
			for _, n := range entityNames(res, 0) {
				if n == prev {
					t.Errorf("round %d: %q survived after being replaced", round, prev)
				}
			}
		}
	}
}

func TestSearchResultsCarryCleanNames(t *testing.T) {
	dir := t.TempDir()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), gateCorpus())
	si := buildSearchIndex(t, dir, cache, nil)

	res, err := si.Search(context.Background(), "config", 20)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(res) == 0 {
		t.Fatal("no results — the test would pass vacuously")
	}

	for _, r := range res {
		t.Logf("%-8s %-14s %q", r.Type, r.Path, r.Name)
		if strings.ContainsRune(r.Name, ' ') {
			t.Errorf("result name %q contains a space: the indexed split is leaking into the "+
				"displayed name", r.Name)
		}
	}
}

// TestHybridSearchDelegatesToKeywordsWhenTheIndexHasNoEmbeddings guards the degrade.
//
// The engine's RRF rank consumes the vector channel's `_distance` column, which no row can
// produce when every embedding is NULL — the failure surfaces as a query-planner error about
// a missing column, not as an actionable message. So the binary question (has vectors, yes
// or no) is recorded at build time, and a hybrid query against a vector-less index must
// answer via keywords instead.
func TestHybridSearchDelegatesToKeywordsWhenTheIndexHasNoEmbeddings(t *testing.T) {
	dir := t.TempDir()
	cache := hybridScaleFixture(t)
	si := buildSearchIndex(t, dir, cache, nil)

	vec := targetVector()
	res, err := si.HybridSearch(context.Background(), "evictOldestStaged", vec, 10)
	if err != nil {
		t.Fatalf("hybrid search with no embeddings errored instead of degrading: %v", err)
	}
	if len(res) == 0 {
		t.Fatal("degraded hybrid search returned nothing the keyword half would have found")
	}
	found := false
	for _, r := range res {
		if r.Name == "evictOldestStaged" {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("degraded hybrid search did not return the sought entity: %+v", res)
	}

	keyword, err := si.Search(context.Background(), "evictOldestStaged", 10)
	if err != nil {
		t.Fatalf("keyword search: %v", err)
	}
	if len(res) != len(keyword) {
		t.Errorf("hybrid-degraded had %d results, keyword-only %d — the degrade must reproduce Search",
			len(res), len(keyword))
	}
}
