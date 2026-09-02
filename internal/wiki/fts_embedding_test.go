//go:build lancedb

package wiki

import (
	"context"
	"math"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// The embedding half of fts.go was the part fts_db_test.go left uncovered:
// PendingEmbeddings, SetChunkVector, EmbeddingStats, SemanticSearch and
// HybridSearch. It looked like it needed a model, and it does not — these
// functions take and return vectors, so synthetic ones exercise every branch.
// What is being tested is the storage and ranking, not the quality of an
// embedding.
//
// Like the rest of that file these fail rather than skip when the vector
// extension is missing, since a binary that cannot load it has a broken wiki.

// unitVec builds a deterministic unit vector pointing mostly along one axis, so
// two vectors built from different axes are far apart and two from the same axis
// are close. That is all the geometry these tests need.
func unitVec(axis int) []float32 {
	v := make([]float32, ai.EmbeddingDimensions)
	for i := range v {
		v[i] = 0.01
	}
	if axis < len(v) {
		v[axis] = 1
	}
	var norm float64
	for _, x := range v {
		norm += float64(x) * float64(x)
	}
	norm = math.Sqrt(norm)
	for i := range v {
		v[i] = float32(float64(v[i]) / norm)
	}
	return v
}

// nudge returns a vector near the given axis but not identical to it.
func nudge(axis int) []float32 {
	v := unitVec(axis)
	v[(axis+1)%len(v)] += 0.02
	return v
}

// slugAxis assigns each test chunk its own axis so a query aimed at one is
// unambiguously nearer to it than to the others.
var slugAxis = map[string]int{
	"autenticacao": 0,
	"indexacao":    1,
	"implantacao":  2,
}

// embeddedTestDB rebuilds the standard fixture and embeds every chunk.
func embeddedTestDB(t *testing.T) *WikiDB {
	t.Helper()
	db := rebuiltTestDB(t)

	pending, err := db.PendingEmbeddings(context.Background())
	if err != nil {
		t.Fatalf("PendingEmbeddings: %v", err)
	}
	if len(pending) == 0 {
		t.Fatal("nothing pending right after a rebuild — the embedding queue is not being filled")
	}
	for _, c := range pending {
		axis, ok := slugAxis[c.Slug]
		if !ok {
			continue
		}
		if err := db.SetChunkVector(context.Background(), c.Slug, unitVec(axis)); err != nil {
			t.Fatalf("SetChunkVector(%s): %v", c.Slug, err)
		}
	}
	return db
}

// The queue is what an embedding worker consumes: everything indexed and not yet
// embedded, and nothing else.
func TestWikiDBPendingEmbeddingsDrains(t *testing.T) {
	db := rebuiltTestDB(t)

	before, err := db.PendingEmbeddings(context.Background())
	if err != nil {
		t.Fatalf("PendingEmbeddings: %v", err)
	}
	if len(before) != len(testChunks()) {
		t.Fatalf("%d chunks pending, want %d", len(before), len(testChunks()))
	}

	embedded, total := db.EmbeddingStats(context.Background())
	if embedded != 0 || total != len(testChunks()) {
		t.Errorf("stats before embedding: %d/%d, want 0/%d", embedded, total, len(testChunks()))
	}

	first := before[0]
	if err := db.SetChunkVector(context.Background(), first.Slug, unitVec(0)); err != nil {
		t.Fatalf("SetChunkVector: %v", err)
	}

	after, err := db.PendingEmbeddings(context.Background())
	if err != nil {
		t.Fatalf("PendingEmbeddings after insert: %v", err)
	}
	if len(after) != len(before)-1 {
		t.Errorf("%d pending after embedding one, want %d", len(after), len(before)-1)
	}
	for _, c := range after {
		if c.Slug == first.Slug {
			t.Errorf("chunk %s is still queued after being embedded", c.Slug)
		}
	}

	embedded, total = db.EmbeddingStats(context.Background())
	if embedded != 1 || total != len(testChunks()) {
		t.Errorf("stats after embedding one: %d/%d, want 1/%d", embedded, total, len(testChunks()))
	}
}

// Nearest-neighbour ranking has to put the chunk whose vector the query is
// aimed at first, or the semantic pass is noise.
func TestWikiDBSemanticSearchRanksNearest(t *testing.T) {
	db := embeddedTestDB(t)

	for slug, axis := range slugAxis {
		res, err := db.SemanticSearch(context.Background(), nudge(axis), 3)
		if err != nil {
			t.Fatalf("SemanticSearch aimed at %s: %v", slug, err)
		}
		if len(res) == 0 {
			t.Fatalf("SemanticSearch aimed at %s returned nothing", slug)
		}
		if res[0].Slug != slug {
			t.Errorf("query aimed at %s ranked %q first; full order %v",
				slug, res[0].Slug, slugsOf(res))
		}
	}
}

func TestWikiDBSemanticSearchRespectsTopK(t *testing.T) {
	db := embeddedTestDB(t)

	res, err := db.SemanticSearch(context.Background(), nudge(0), 1)
	if err != nil {
		t.Fatalf("SemanticSearch: %v", err)
	}
	if len(res) != 1 {
		t.Errorf("topK=1 returned %d results", len(res))
	}
}

// A chunk with no vector must not appear in a semantic result, however close its
// text is.
func TestWikiDBSemanticSearchIgnoresUnembedded(t *testing.T) {
	db := rebuiltTestDB(t)

	pending, err := db.PendingEmbeddings(context.Background())
	if err != nil {
		t.Fatalf("PendingEmbeddings: %v", err)
	}
	var embeddedSlug string
	for _, c := range pending {
		if c.Slug == "autenticacao" {
			if err := db.SetChunkVector(context.Background(), c.Slug, unitVec(0)); err != nil {
				t.Fatalf("SetChunkVector: %v", err)
			}
			embeddedSlug = c.Slug
		}
	}
	if embeddedSlug == "" {
		t.Fatal("fixture changed: autenticacao is not in the pending set")
	}

	res, err := db.SemanticSearch(context.Background(), nudge(1), 5)
	if err != nil {
		t.Fatalf("SemanticSearch: %v", err)
	}
	for _, r := range res {
		if r.Slug != embeddedSlug {
			t.Errorf("%s came back from a semantic search without ever being embedded", r.Slug)
		}
	}
}

// Hybrid has to work when only one side can contribute, which is the common case
// early in a run: text is indexed immediately, vectors arrive later.
func TestWikiDBHybridSearchCombinesBothPasses(t *testing.T) {
	db := embeddedTestDB(t)

	t.Run("text only", func(t *testing.T) {
		res, err := db.HybridSearch(context.Background(), "credenciais", nil, 5)
		if err != nil {
			t.Fatalf("HybridSearch: %v", err)
		}
		if len(res) == 0 {
			t.Fatal("a term present in the corpus returned nothing with no vector")
		}
	})

	t.Run("vector only", func(t *testing.T) {
		res, err := db.HybridSearch(context.Background(), "", nudge(2), 5)
		if err != nil {
			t.Fatalf("HybridSearch: %v", err)
		}
		if len(res) == 0 {
			t.Fatal("a vector aimed at an embedded chunk returned nothing with no text")
		}
	})

	t.Run("both, agreeing", func(t *testing.T) {
		res, err := db.HybridSearch(context.Background(), "credenciais", nudge(0), 5)
		if err != nil {
			t.Fatalf("HybridSearch: %v", err)
		}
		if len(res) == 0 {
			t.Fatal("both passes pointing at the same chunk returned nothing")
		}
		if res[0].Slug != "autenticacao" {
			t.Errorf("both passes aimed at autenticacao, ranked %q first; order %v",
				res[0].Slug, slugsOf(res))
		}
	})

	t.Run("neither", func(t *testing.T) {
		res, err := db.HybridSearch(context.Background(), "", nil, 5)
		if err != nil {
			t.Errorf("empty query and no vector should be empty, not an error: %v", err)
		}
		if len(res) != 0 {
			t.Errorf("empty query and no vector returned %d results", len(res))
		}
	})
}

// A sync must not leave vectors pointing at chunks that no longer exist.
func TestWikiDBSyncRemovesDeletedEmbeddingState(t *testing.T) {
	db := embeddedTestDB(t)

	embedded, _ := db.EmbeddingStats(context.Background())
	if embedded == 0 {
		t.Fatal("precondition: chunks should be embedded")
	}

	kept := testChunks()[1:]
	if err := db.Sync(context.Background(), kept, nil, nil); err != nil {
		t.Fatalf("Sync: %v", err)
	}

	_, total := db.EmbeddingStats(context.Background())
	if total != len(kept) {
		t.Errorf("total after rebuild is %d, want %d", total, len(kept))
	}

	res, err := db.SemanticSearch(context.Background(), nudge(0), 5)
	if err != nil {
		t.Fatalf("SemanticSearch after rebuild: %v", err)
	}
	for _, r := range res {
		if r.Slug == "autenticacao" {
			t.Error("a chunk deleted by the sync is still reachable through its vector")
		}
	}
}
