//go:build lancedb

package ast

import (
	"context"
	"strings"
	"testing"
)

// TestSearchOrderIsDeterministic pins down a defect found while measuring the
// prefix-index gap: identical corpus and identical query returned different top-1
// results across runs (3/5 validateSchema, 2/5 PKG_VALIDACAO_PAGAMENTO for "valid").
//
// The cause is the index BUILD, not the query. Repeated Search calls against one index
// agree; it is rebuilding that shuffles the answer, because the rebuild iterates the parse
// cache — a map — so rows land in the index in a different order each time, and tied BM25
// scores are broken by insertion order. The ranking therefore depended silently on map
// iteration.
//
// A query-time tie-breaker is the robust fix rather than merely sorting the build:
// incremental updates append changed files, so physical order diverges from a full rebuild
// as soon as anything is edited, even if the build were ordered. Found on the SQLite index
// and carried over deliberately — the same fusion code, and the same exposure, backs the
// LadybugDB index.
//
// Why it matters more than a ranking wobble: search is served to an agent over MCP,
// so an irreproducible top-1 makes the same question yield different work. Ranking
// may be debatable; ordering must at least be a function of the data.
func TestSearchOrderIsDeterministic(t *testing.T) {
	corpus := prefixCorpus()
	queries := []string{"valid", "config", "conf", "database", "schema"}

	// Independent indexes over the same corpus. Each gets its own ShardCache, whose
	// map iteration order differs, which is what reproduces the cross-run flip
	// inside a single test.
	assertStableAcrossRebuilds(t, corpus, queries, func(si *SearchIndex, q string) ([]SearchResult, error) {
		return si.Search(context.Background(), q, 10)
	})
}

// assertStableAcrossRebuilds builds the same corpus repeatedly and requires each
// query's result order to be identical every time.
func assertStableAcrossRebuilds(t *testing.T, corpus []gateEntity, queries []string,
	search func(*SearchIndex, string) ([]SearchResult, error)) {
	t.Helper()

	const rebuilds = 8
	first := make(map[string][]string, len(queries))
	unstable := make(map[string]bool, len(queries))

	for build := 0; build < rebuilds; build++ {
		si := buildSearchIndexFrom(t, t.TempDir(), corpus)
		for _, query := range queries {
			res, err := search(si, query)
			if err != nil {
				t.Fatalf("search %q: %v", query, err)
			}
			got := make([]string, 0, len(res))
			for _, r := range res {
				got = append(got, r.Type+":"+r.Path+":"+r.Name)
			}
			if len(got) == 0 {
				t.Fatalf("query %q returned nothing — the test would pass vacuously", query)
			}
			if build == 0 {
				first[query] = got
				continue
			}
			if strings.Join(got, "|") != strings.Join(first[query], "|") {
				unstable[query] = true
				t.Errorf("query %q depends on index build order; rebuild %d differs from the first:\n first: %v\n got:   %v",
					query, build, first[query], got)
			}
		}
	}

	for _, query := range queries {
		if unstable[query] {
			t.Logf("%-10s %d results, UNSTABLE across %d rebuilds", query, len(first[query]), rebuilds)
			continue
		}
		t.Logf("%-10s %d results, stable across %d rebuilds", query, len(first[query]), rebuilds)
	}
}

// TestHybridSearchOrderIsDeterministic covers the same defect on the hybrid path,
// which builds its ranking with the identical map-then-sort.Slice shape.
func TestHybridSearchOrderIsDeterministic(t *testing.T) {
	// Rebuilds, not repeated calls on one index: repeated calls agree even with the
	// defect present, so that form of the test passed vacuously.
	assertStableAcrossRebuilds(t, prefixCorpus(), []string{"valid", "config", "database"},
		func(si *SearchIndex, q string) ([]SearchResult, error) {
			return si.HybridSearch(context.Background(), q, nil, 10)
		})
}
