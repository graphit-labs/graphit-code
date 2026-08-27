//go:build lancedb

package wiki

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// The wiki index on LanceDB, tested against the real engine.
//
// A wiki has no graph, so there is one store and the cross-reference walk is Go over one-hop
// lookups — which is what these tests exercise rather than a graph traversal that does not exist.

func newWikiForTest(t *testing.T) *WikiDB {
	t.Helper()
	db, err := OpenWikiDBAt(context.Background(),
		lancestore.Config{URI: t.TempDir() + "/index.lance"})
	if err != nil {
		t.Fatalf("open wiki index: %v", err)
	}
	t.Cleanup(func() { _ = db.Close() })
	return db
}

func lanceChunk(slug, title, summary, body string) WikiChunk {
	return WikiChunk{
		Slug: slug, Title: title, Summary: summary, Body: body,
		DocType: "reference", Source: slug + ".md",
		ContentHash: "h-" + slug, WordCount: len(strings.Fields(body)),
		ClusterID: -1, Confidence: 1,
	}
}

func lanceSlugsOf(rs []WikiSearchResult) []string {
	out := make([]string, 0, len(rs))
	for _, r := range rs {
		out = append(out, r.Slug)
	}
	return out
}

func hasSlug(hay []string, needle string) bool {
	for _, h := range hay {
		if h == needle {
			return true
		}
	}
	return false
}

// ---------- rebuild and search ----------

func TestLanceWikiRebuildAndKeywordSearch(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	chunks := []WikiChunk{
		lanceChunk("hub-s3-object-layout", "Hub S3 Object Layout",
			"How artifacts are laid out in the bucket.",
			"The registry lives at the prefix root and every artifact gets its own key."),
		lanceChunk("memory-scopes", "Memory Scopes",
			"Project and user scopes, and how they are pulled.",
			"A scope is pulled on first use and merged rather than replaced."),
	}
	if err := db.Rebuild(ctx, chunks, nil, nil, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	if !db.HasContent(ctx) {
		t.Error("HasContent is false after a rebuild that wrote two chunks")
	}
	got, err := db.Search(ctx, "artifact layout bucket", 5)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if !hasSlug(lanceSlugsOf(got), "hub-s3-object-layout") {
		t.Errorf("body terms did not find their page: %v", lanceSlugsOf(got))
	}

	got, err = db.Search(ctx, "memory scopes", 5)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if !hasSlug(lanceSlugsOf(got), "memory-scopes") {
		t.Errorf("the title did not find its page: %v", lanceSlugsOf(got))
	}
}

// The slug carries the title of a page whose title may have been written later, so a query that
// matches only the slug has to land.
func TestLanceWikiSlugIsSearchable(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)
	if err := db.Rebuild(ctx, []WikiChunk{
		lanceChunk("icebug-remote-graph", "Untitled", "", "Body with no useful words."),
		lanceChunk("something-else", "Other", "", "Different body entirely."),
	}, nil, nil, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	got, err := db.Search(ctx, "icebug remote", 5)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if !hasSlug(lanceSlugsOf(got), "icebug-remote-graph") {
		t.Errorf("the slug's words did not find the page: %v", lanceSlugsOf(got))
	}
}

// A rebuild is all-or-nothing: a page that left the sources must not survive it.
func TestLanceWikiRebuildDropsRemovedPages(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	if err := db.Rebuild(ctx, []WikiChunk{
		lanceChunk("goes-away", "Goes Away", "", "This page is about departure."),
	}, nil, nil, nil); err != nil {
		t.Fatalf("first rebuild: %v", err)
	}
	if err := db.Rebuild(ctx, []WikiChunk{
		lanceChunk("stays", "Stays", "", "This page is about persistence."),
	}, nil, nil, nil); err != nil {
		t.Fatalf("second rebuild: %v", err)
	}

	got, _ := db.Search(ctx, "departure", 10)
	if hasSlug(lanceSlugsOf(got), "goes-away") {
		t.Error("a page absent from the sources survived a rebuild")
	}
	got, _ = db.Search(ctx, "persistence", 10)
	if !hasSlug(lanceSlugsOf(got), "stays") {
		t.Errorf("the surviving page is missing: %v", lanceSlugsOf(got))
	}
}

// ---------- cross-references ----------

func TestLanceWikiXRefsAreBidirectionalAndWalkDepth(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	chunks := []WikiChunk{
		lanceChunk("a", "Page A", "", "Alpha."),
		lanceChunk("b", "Page B", "", "Beta."),
		lanceChunk("c", "Page C", "", "Gamma."),
	}
	xrefs := map[string][]string{"a": {"b"}, "b": {"c"}}
	if err := db.Rebuild(ctx, chunks, xrefs, nil, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	// Depth 1 from a reaches b only.
	one, err := db.FindXRefs(ctx, "a", 1)
	if err != nil {
		t.Fatalf("xrefs: %v", err)
	}
	var oneSlugs []string
	for _, r := range one {
		oneSlugs = append(oneSlugs, r.Slug)
	}
	if !hasSlug(oneSlugs, "b") {
		t.Errorf("depth 1 from a did not reach b: %v", oneSlugs)
	}
	if hasSlug(oneSlugs, "c") {
		t.Errorf("depth 1 from a reached c, which is two hops away: %v", oneSlugs)
	}

	// Depth 2 reaches c.
	two, err := db.FindXRefs(ctx, "a", 2)
	if err != nil {
		t.Fatalf("xrefs: %v", err)
	}
	var twoSlugs []string
	for _, r := range two {
		twoSlugs = append(twoSlugs, r.Slug)
	}
	if !hasSlug(twoSlugs, "c") {
		t.Errorf("depth 2 from a did not reach c: %v", twoSlugs)
	}

	// b sees a as INBOUND, which is what makes "what links here" answerable.
	fromB, err := db.FindXRefs(ctx, "b", 1)
	if err != nil {
		t.Fatalf("xrefs: %v", err)
	}
	var inbound bool
	for _, r := range fromB {
		if r.Slug == "a" && r.Direction == "inbound" {
			inbound = true
		}
	}
	if !inbound {
		t.Errorf("b does not see a as an inbound reference: %+v", fromB)
	}
}

// A slug with an apostrophe must not change which rows match. An unescaped quote in a filter does
// not fail — it silently matches the wrong set, which is the worst kind of bug to ship.
func TestLanceWikiSlugWithAQuoteIsFilteredSafely(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	tricky := "what's-new"
	if err := db.Rebuild(ctx, []WikiChunk{
		lanceChunk(tricky, "What's New", "", "Recent changes."),
		lanceChunk("other", "Other", "", "Unrelated."),
	}, map[string][]string{tricky: {"other"}}, nil, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	refs, err := db.FindXRefs(ctx, tricky, 1)
	if err != nil {
		t.Fatalf("xrefs for a quoted slug: %v", err)
	}
	if len(refs) != 1 || refs[0].Slug != "other" {
		t.Errorf("a slug containing an apostrophe resolved to %+v, want exactly [other]", refs)
	}
}

// ---------- browse ----------

func TestLanceWikiBrowseFiltersAndIsOrdered(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	specs := lanceChunk("spec-b", "Spec B", "", "Body.")
	specs.DocType = "spec"
	specA := lanceChunk("spec-a", "Spec A", "", "Body.")
	specA.DocType = "spec"
	guide := lanceChunk("guide-1", "Guide", "", "Body.")
	guide.DocType = "guide"

	if err := db.Rebuild(ctx, []WikiChunk{specs, specA, guide}, nil, nil, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	all, err := db.Browse(ctx, BrowseFilter{ClusterID: -1})
	if err != nil {
		t.Fatalf("browse: %v", err)
	}
	if len(all) != 3 {
		t.Errorf("an unfiltered browse returned %d entries, want 3", len(all))
	}

	onlySpecs, err := db.Browse(ctx, BrowseFilter{DocType: "spec", ClusterID: -1})
	if err != nil {
		t.Fatalf("browse by type: %v", err)
	}
	if len(onlySpecs) != 2 {
		t.Fatalf("browse by doc_type returned %d, want 2: %+v", len(onlySpecs), onlySpecs)
	}
	// A filter query has no ranking channel, so the order is imposed here and must be stable.
	if onlySpecs[0].Slug != "spec-a" || onlySpecs[1].Slug != "spec-b" {
		t.Errorf("browse is not ordered by slug: %v, %v", onlySpecs[0].Slug, onlySpecs[1].Slug)
	}
}

// ---------- sync log ----------

// The sync log is the history OF rebuilds, so a rebuild must not erase it. Clearing it every time
// would leave it permanently one entry long, which reads as "this wiki has only ever synced once".
func TestLanceWikiSyncLogSurvivesARebuild(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	first := &SyncLogEntry{Timestamp: "2026-08-01T00:00:00Z", TotalDocs: 1, ArticlesWritten: 1}
	if err := db.Rebuild(ctx, []WikiChunk{lanceChunk("a", "A", "", "x")}, nil, first, nil); err != nil {
		t.Fatalf("first rebuild: %v", err)
	}
	second := &SyncLogEntry{Timestamp: "2026-08-02T00:00:00Z", TotalDocs: 2, ArticlesWritten: 2,
		Added: []string{"b"}}
	if err := db.Rebuild(ctx, []WikiChunk{lanceChunk("a", "A", "", "x"), lanceChunk("b", "B", "", "y")},
		nil, second, nil); err != nil {
		t.Fatalf("second rebuild: %v", err)
	}

	log, err := db.QuerySyncLog(ctx, 10)
	if err != nil {
		t.Fatalf("query sync log: %v", err)
	}
	if len(log) != 2 {
		t.Fatalf("sync log has %d entries after two rebuilds, want 2: %+v", len(log), log)
	}
	if log[0].Timestamp != "2026-08-02T00:00:00Z" {
		t.Errorf("the log is not newest-first: %v", log[0].Timestamp)
	}
	if len(log[0].Added) != 1 || log[0].Added[0] != "b" {
		t.Errorf("the JSON-encoded slug list did not round-trip: %+v", log[0].Added)
	}
}

// ---------- embeddings ----------

func TestLanceWikiEmbeddingsAttachBySlugAndAreCounted(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	if err := db.Rebuild(ctx, []WikiChunk{
		lanceChunk("a", "A", "", "First."), lanceChunk("b", "B", "", "Second."),
	}, nil, nil, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	pending, err := db.PendingEmbeddings(ctx)
	if err != nil {
		t.Fatalf("pending: %v", err)
	}
	if len(pending) != 2 {
		t.Errorf("pending embeddings = %d, want 2", len(pending))
	}

	vec := make([]float32, ai.EmbeddingDimensions)
	vec[0] = 1
	if err := db.SetChunkVector(ctx, "a", vec); err != nil {
		t.Fatalf("set vector: %v", err)
	}

	embedded, total := db.EmbeddingStats(ctx)
	if embedded != 1 || total != 2 {
		t.Errorf("embedding stats = %d/%d, want 1/2", embedded, total)
	}

	pending, err = db.PendingEmbeddings(ctx)
	if err != nil {
		t.Fatalf("pending after write: %v", err)
	}
	if len(pending) != 1 || pending[0].Slug != "b" {
		t.Errorf("pending after embedding a = %+v, want just b", slugsOfChunks(pending))
	}

	// Attaching a vector must not duplicate the chunk. Upsert keyed by slug is what prevents it.
	if _, slugs, _, _, err := db.Stats(ctx); err != nil {
		t.Fatal(err)
	} else if slugs != 2 {
		t.Errorf("after attaching an embedding the wiki has %d chunks, want 2", slugs)
	}

	stored, err := db.StoredEmbeddings(ctx)
	if err != nil {
		t.Fatalf("stored embeddings: %v", err)
	}
	if len(stored) != 1 || stored[0].ContentHash != "h-a" {
		t.Errorf("stored embeddings = %+v, want one for h-a", stored)
	}
}

func slugsOfChunks(rs []chunkRow) []string {
	out := make([]string, 0, len(rs))
	for _, r := range rs {
		out = append(out, r.Slug)
	}
	return out
}

// A rebuild carries cached vectors back in, keyed by content hash, so a page whose text did not
// change is not re-embedded.
func TestLanceWikiRebuildReusesCachedVectors(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	vec := make([]float32, ai.EmbeddingDimensions)
	vec[7] = 1
	cache := EmbeddingCache{"h-a": vec}

	if err := db.Rebuild(ctx, []WikiChunk{lanceChunk("a", "A", "", "Text.")}, nil, nil, cache); err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	if embedded, total := db.EmbeddingStats(ctx); embedded != 1 || total != 1 {
		t.Errorf("cached vector was not carried into the rebuild: %d/%d", embedded, total)
	}
}

// ---------- hybrid ----------

func TestLanceWikiHybridSearchUsesTheEngineFusion(t *testing.T) {
	ctx := context.Background()
	db := newWikiForTest(t)

	vecA := make([]float32, ai.EmbeddingDimensions)
	vecA[0] = 1
	vecB := make([]float32, ai.EmbeddingDimensions)
	vecB[1] = 1

	if err := db.Rebuild(ctx, []WikiChunk{
		lanceChunk("alpha", "Alpha Page", "", "Content about registries."),
		lanceChunk("beta", "Beta Page", "", "Content about scopes."),
	}, nil, nil, EmbeddingCache{"h-alpha": vecA, "h-beta": vecB}); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	got, err := db.HybridSearch(ctx, "registries", vecA, 5)
	if err != nil {
		t.Fatalf("hybrid: %v", err)
	}
	if len(got) == 0 {
		t.Fatal("a hybrid query returned nothing")
	}
	if got[0].Slug != "alpha" {
		t.Errorf("hybrid top hit is %q; both channels pointed at alpha", got[0].Slug)
	}
}

// A published wiki is read-only, and the write paths have to refuse rather than half-apply.
func TestLanceWikiRebuildRefusedWhenRemote(t *testing.T) {
	// Constructed directly against an s3:// URI so no bucket is needed: Open marks the store
	// remote from the scheme, and every write consults that flag.
	db, err := OpenWikiDBAt(context.Background(), lancestore.Config{URI: "s3://example/wiki"})
	if err != nil {
		t.Skipf("cannot open a remote store in this environment: %v", err)
	}
	defer func() { _ = db.Close() }()

	if !db.Remote() {
		t.Fatal("a store opened on s3:// is not marked remote")
	}
	err = db.Rebuild(context.Background(), []WikiChunk{lanceChunk("a", "A", "", "x")}, nil, nil, nil)
	if err == nil {
		t.Fatal("rebuilding a published wiki was allowed")
	}
	if !strings.Contains(fmt.Sprint(err), "read-only") {
		t.Errorf("the refusal does not name the reason: %v", err)
	}
}
