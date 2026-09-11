//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func targetVector() []float32 {
	v := make([]float32, ai.EmbeddingDimensions)
	v[0] = 1
	return v
}

func otherVector() []float32 {
	v := make([]float32, ai.EmbeddingDimensions)
	for i := range v {
		v[i] = 0.01
	}
	return v
}

func entityVectors() func(string, string) []float32 {
	return func(_, uid string) []float32 {
		if strings.HasSuffix(uid, "evictOldestStaged") {
			return targetVector()
		}
		return otherVector()
	}
}

func hybridScaleFixture(t *testing.T) *ShardCache {
	t.Helper()
	entries := []*parseCacheEntry{
		entryWith("internal/hub/s3_store.go",
			"package hub\n\n// evictOldestStaged drops the oldest staged events.\nfunc evictOldestStaged() {}\n",
			cachedEntity{Name: "evictOldestStaged", Label: "Method"}),
	}
	for i := 0; i < 4; i++ {
		rel := fmt.Sprintf("internal/ast/mentions_%d_test.go", i)
		entries = append(entries, entryWith(rel,
			"package ast\n\n// a test that talks about evictOldestStaged in passing\n",
			cachedEntity{Name: fmt.Sprintf("TestUnrelated%d", i)}))
	}
	return newShardCacheForTest(t, entries...)
}

// TestHybridSearchDoesNotLetFileScoresOutrankEntities is the guard. It FAILS on the code that
// sorted both passes together, returning a File at rank one for a query that names a method.
func TestHybridSearchDoesNotLetFileScoresOutrankEntities(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	scaleCache := hybridScaleFixture(t)
	if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, scaleCache, entityVectors())

	vec := targetVector()

	results, err := idx.HybridSearch(ctx, "evictOldestStaged", vec, 0)
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("hybrid search over a populated index returned nothing")
	}

	for i, r := range results {
		t.Logf("result[%d] type=%-8s score=%-12.6f %s (%s)", i, r.Type, r.RelevanceScore, r.Name, r.Path)
	}

	if results[0].Type == LabelFile {
		t.Errorf("rank one is a File (%s, score %g) for a query naming a method — the file pass's raw BM25 outranked the entity pass's fused score",
			results[0].Path, results[0].RelevanceScore)
	}
	if results[0].Name != "evictOldestStaged" {
		t.Errorf("rank one is %q (%s), want the entity evictOldestStaged", results[0].Name, results[0].Type)
	}
}

// A file result must still be REACHABLE — the fix is precedence, not exclusion. A query that
// matches files has to keep returning them, after the entities.
func TestHybridSearchStillReturnsFilesAfterEntities(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	scaleCache := hybridScaleFixture(t)
	if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, scaleCache, entityVectors())

	vec := targetVector()
	results, err := idx.HybridSearch(ctx, "evictOldestStaged", vec, 0)
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}

	firstFile := -1
	lastEntity := -1
	for i, r := range results {
		if r.Type == LabelFile {
			if firstFile < 0 {
				firstFile = i
			}
		} else {
			lastEntity = i
		}
	}
	if firstFile < 0 {
		t.Fatal("no file result at all — precedence must not become exclusion")
	}
	if firstFile < lastEntity {
		t.Errorf("a file at rank %d precedes an entity at rank %d: the two lists are interleaved", firstFile, lastEntity)
	}
}

// Both lists are ordered by their own scores, descending. This is only safe because lancestore
// stopped collapsing _score and _relevance_score into one field: while it did, the hybrid score
// was picked by map iteration order and this assertion would have been meaningless.
func TestBothListsStayOrderedByTheirOwnScore(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	scaleCache := hybridScaleFixture(t)
	if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, scaleCache, entityVectors())

	results, err := idx.HybridSearch(ctx, "evictOldestStaged", targetVector(), 0)
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}

	prev := map[bool]float64{}
	seen := map[bool]bool{}
	for i, r := range results {
		isFile := r.Type == LabelFile
		if seen[isFile] && r.RelevanceScore > prev[isFile] {
			t.Errorf("result[%d] (file=%v) scores %g above the previous %g in the same list",
				i, isFile, r.RelevanceScore, prev[isFile])
		}
		prev[isFile] = r.RelevanceScore
		seen[isFile] = true
	}
	if !seen[true] || !seen[false] {
		t.Fatalf("both lists must be present for this to assert anything: files=%v entities=%v",
			seen[true], seen[false])
	}
}

func TestHybridKeepsTheEnginesOrderRatherThanItsScore(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	scaleCache := hybridScaleFixture(t)
	if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, scaleCache, entityVectors())

	raw, err := idx.entities.Search(ctx, lancestore.Query{
		Text: LanceQueryText("evictOldestStaged"), TextColumn: lanceBodyColumn,
		Vector: targetVector(), VectorColumn: lanceVectorColumn, Limit: 10,
	})
	if err != nil {
		t.Fatalf("raw entity search: %v", err)
	}
	if len(raw) == 0 {
		t.Fatal("the engine returned nothing")
	}

	results, err := idx.HybridSearch(ctx, "evictOldestStaged", targetVector(), 0)
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}

	var gotEntities []string
	for _, r := range results {
		if r.Type != LabelFile {
			gotEntities = append(gotEntities, r.Name)
		}
	}
	for i, h := range raw {
		want, _ := h.Row["name"].(string)
		if i >= len(gotEntities) {
			t.Fatalf("the entity list is shorter than the engine's answer: %d vs %d", len(gotEntities), len(raw))
		}
		if gotEntities[i] != want {
			t.Errorf("entity rank %d is %q, but the engine put %q there", i, gotEntities[i], want)
		}
	}
}

// topK must still cap the answer, and the cap must not be spent on files while entities are
// dropped.
func TestHybridSearchTopKPrefersEntities(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	scaleCache := hybridScaleFixture(t)
	if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, scaleCache, entityVectors())

	vec := targetVector()
	results, err := idx.HybridSearch(ctx, "evictOldestStaged", vec, 2)
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}
	if len(results) > 2 {
		t.Fatalf("topK=2 returned %d results", len(results))
	}
	for i, r := range results {
		if r.Type == LabelFile {
			t.Errorf("result[%d] is a File under topK=2 while entities matched the query", i)
		}
	}
}

func TestHybridTopResultAndSetAreStableAcrossRebuilds(t *testing.T) {
	ctx := context.Background()
	const rebuilds = 8
	var firstTop string
	var firstSet []string

	for build := 0; build < rebuilds; build++ {
		idx := newLanceIndexForTest(t)
		scaleCache := hybridScaleFixture(t)
		if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
			t.Fatalf("rebuild %d: %v", build, err)
		}
		results, err := idx.HybridSearch(ctx, "evictOldestStaged", targetVector(), 0)
		if err != nil {
			t.Fatalf("hybrid search on rebuild %d: %v", build, err)
		}
		if len(results) == 0 {
			t.Fatalf("rebuild %d returned nothing — the test would pass vacuously", build)
		}

		top := results[0].Type + ":" + results[0].Path + ":" + results[0].Name
		set := make([]string, 0, len(results))
		for _, r := range results {
			set = append(set, r.Type+":"+r.Path+":"+r.Name)
		}
		sort.Strings(set)

		if build == 0 {
			firstTop, firstSet = top, set
			continue
		}
		if top != firstTop {
			t.Errorf("rebuild %d ranks %q first; the first rebuild ranked %q", build, top, firstTop)
		}
		if strings.Join(set, "|") != strings.Join(firstSet, "|") {
			t.Errorf("rebuild %d returned a different SET:\n first: %v\n got:   %v", build, firstSet, set)
		}
	}
	t.Logf("top-1 %q and a %d-row set stable across %d rebuilds", firstTop, len(firstSet), rebuilds)
}

func TestKeywordSearchStillRanksTheEntityFirst(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	if err := idx.RebuildFromCache(ctx, hybridScaleFixture(t), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	results, err := idx.Search(ctx, "evictOldestStaged", 0)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("keyword search returned nothing")
	}
	if results[0].Name != "evictOldestStaged" {
		t.Errorf("rank one is %q (%s), want the entity evictOldestStaged", results[0].Name, results[0].Type)
	}
}

func TestFilePassCarriesTheRerankConfig(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	if err := idx.RebuildFromCache(ctx, hybridScaleFixture(t), nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	spy := &recordingReranker{}
	_, err := idx.search(ctx, lancestore.Query{
		Text: LanceQueryText("evictOldestStaged"), TextColumn: lanceBodyColumn,
		Rerank: lancestore.RerankConfig{Reranker: spy},
	}, 0)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if spy.calls < 2 {
		t.Errorf("the reranker ran %d time(s); both the entity pass and the file pass must reach it", spy.calls)
	}
}

type recordingReranker struct{ calls int }

func (r *recordingReranker) Name() string { return "spy" }

func (r *recordingReranker) Rerank(_ context.Context, _ string, hits []lancestore.Hit) ([]lancestore.Hit, error) {
	r.calls++
	return hits, nil
}
