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

// The two passes of a search do NOT produce scores on one scale, and this is the file that pins
// it.
//
// The reported symptom was `ast query --hybrid "evictOldestStaged"` answering with five Files and
// no entity, for a method that is indexed. The diagnosis it arrived with — that the two passes are
// BM25 over corpora of different sizes, so file IDF dominates entity IDF — is WRONG, and measuring
// it was worth more than the fix. Against the real index of this project (61,446 entities, 770
// files), the same query on the keyword channel gives:
//
//	entity pass: 156.4 (the method)      file pass: 29.6 (the file declaring it)
//
// The entity wins by 5x. Raw BM25 was never the problem, which is exactly why every keyword-mode
// gate passed while the CLI returned nothing but files.
//
// What is actually true, and both halves matter:
//
//  1. A hybrid entity pass returns the engine's FUSED score — an RRF sum, ~1/(60+rank), so in the
//     hundredths. The file pass drops the vector channel, because the files table has no embedding
//     column, and returns raw BM25 in the tens. One sort over the concatenation therefore puts
//     every file above every entity. Fixed by precedence, not by normalising the two into one
//     scale — that would be the weighted fusion in Go this project deleted.
//  2. A hybrid row carries BOTH _score and _relevance_score, and lancestore collapsed the two into
//     one field from inside a `for k := range row`. The surviving value was whichever Go's
//     randomised map iteration visited last, so the ENTITY LIST was ordered by a coin toss.
//     Measured: twenty identical queries against one unchanged index returned two different scores
//     for every row.
//
// The discriminator these tests need is therefore the VECTOR CHANNEL, not a big corpus: a fixture
// of five files reproduces both, because tens beat hundredths at any corpus size.

// targetVector is the direction of the entity the query is looking for, and queryVector below is
// the same direction — so the semantic channel AGREES with the keyword channel about which entity
// is wanted.
//
// That agreement is deliberate and it is the second thing this file learned the hard way. An
// earlier version gave every entity ONE shared vector, on the reasoning that only the score's
// SCALE was under test. It is not enough: identical embeddings make the vector channel pure noise
// carried at full confidence, the engine fuses that noise with BM25, and an unrelated entity
// takes rank one. The test then flapped — passing alone, failing in the full suite — which is a
// worse failure than the one it was written to catch.
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

// entityVectors points the sought entity one way and everything else another, so which entity the
// vector channel prefers is not a coin toss.
func entityVectors() func(string, string) []float32 {
	return func(_, uid string) []float32 {
		if strings.HasSuffix(uid, "evictOldestStaged") {
			return targetVector()
		}
		return otherVector()
	}
}

// hybridScaleFixture is one file that declares the sought entity plus four that merely mention
// the term in prose, so the term is rare enough for the file pass to score it highly — which is
// the situation the real corpus produced.
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

	// topK = 0 is what `ast query --hybrid` passes by default ("--top 0 = no limit"), and it is
	// the value that makes the file pass run unconditionally.
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

// The engine's hybrid ORDER has to survive the trip through this package. Sorting the results by
// the engine's own score moved the one entity the query named from rank one to rank four, which is
// the defect this pins.
func TestHybridKeepsTheEnginesOrderRatherThanItsScore(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	scaleCache := hybridScaleFixture(t)
	if err := idx.RebuildFromCache(ctx, scaleCache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	applyVectors(t, idx, scaleCache, entityVectors())

	// What the engine itself returns, before this package touches it.
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

// What the hybrid path guarantees across rebuilds, and what it does NOT — measured, because the
// real hybrid channel had no determinism gate at all before this.
//
// It is not covered by TestHybridSearchOrderIsDeterministic: that one calls
// HybridSearch(..., nil, 10) with a NIL vector, so it degrades to the keyword path and never
// exercises the fusion.
//
// GUARANTEED, and asserted here: the top result and the result SET are the same on every rebuild.
// That is what "the same question yields the same work" needs.
//
// NOT GUARANTEED, and deliberately not asserted: the relative order of rows that are tied on BOTH
// channels. This fixture has four such rows on purpose — identical BM25 (0.353) and identical
// vectors — and their order does permute across rebuilds.
//
// The reason it cannot be fixed the way the keyword path fixes it: sortResultsDeterministic breaks
// EQUAL scores by identity, and on the keyword channel tied rows really do carry equal scores, so
// it engages. On a hybrid query the engine assigns tied rows distinct RRF values — 1/(60+rank),
// differing in the fourth decimal purely because it had to put them in some order — so the scores
// are unequal and the tie-break never runs. Recovering it would mean deciding in Go that two
// engine ranks are "close enough to be a tie", which is ranking policy this package does not own.
// The residual is bounded to rows that carry no signal to distinguish them.
func TestHybridTopResultAndSetAreStableAcrossRebuilds(t *testing.T) {
	ctx := context.Background()
	const rebuilds = 8
	var firstTop string
	var firstSet []string

	for build := 0; build < rebuilds; build++ {
		// A fresh index and a fresh shard cache each time: the map iteration order of the cache
		// is what shuffles insertion order, which is what the original defect rode on.
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

// The keyword-only path was never broken, and the fix must not change what it returns. Both of
// its passes are raw BM25, and on that channel the entity legitimately wins — measured 156.4
// against 29.6 on the real corpus.
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

// The reranker is the third score family, and it is the reason the merge rule cannot be "sort by
// score" even conditionally. When a cross-encoder is wired, the file pass has to be judged by the
// SAME stage as the entity pass — otherwise half the answer is ranked by a model and half by
// BM25, which is the defect above wearing a different scale.
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

// recordingReranker counts how many passes reached the second stage. It returns the set it was
// given, unchanged, because a reranker that alters the set breaks lancestore's contract.
type recordingReranker struct{ calls int }

func (r *recordingReranker) Name() string { return "spy" }

func (r *recordingReranker) Rerank(_ context.Context, _ string, hits []lancestore.Hit) ([]lancestore.Hit, error) {
	r.calls++
	return hits, nil
}
