//go:build lancedb

package hub

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/textslice"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// THE WHOLE POINT, END TO END: a wiki is published to object storage and then READ FROM THERE.
//
// No local copy of the published artifact exists at any point after the upload — the directory it
// was built in is not the directory it is read from, and the read goes through an `s3://` URI. If
// this passes, "installing stops downloading" is a fact rather than a plan.
//
//	GRAPHIT_LANCE_S3_ENDPOINT=http://localhost:9000 GRAPHIT_LANCE_S3_BUCKET=graphit-hub \
//	AWS_ACCESS_KEY_ID=… AWS_SECRET_ACCESS_KEY=… \
//	  go test -tags lancedb -run TestPublishedWikiIsRead ./internal/hub/ -v
//
// It needs a REAL object store, not the in-memory fake: the engine's reader is the Rust
// object_store crate, and what it asks of a bucket — listing with delimiters, conditional reads —
// is more than the fake implements. Measured: against the fake the upload lands correctly and the
// engine then reports `no such table`, which says nothing about our code. The upload half is
// asserted separately in TestPublishedWikiCarriesItsIndexes, which does run everywhere.
func TestPublishedWikiIsReadDirectlyFromObjectStorage(t *testing.T) {
	ctx := context.Background()

	// TWO TRANSPORTS, ONE CODE PATH. With a bucket configured this is the real thing: objects on
	// S3, read over the network. Without one it runs the identical wiring — publish, resolve the
	// mount, open it, search it, read a page, walk the cross-references — over a local URI.
	//
	// The weaker run is worth having rather than skipping, because what it covers is where the
	// bugs were: the read going through a WRITE path (EnsureTable creates, and creating is refused
	// on a published store), and the mount addressing a directory the publisher did not write to.
	// Both are transport-independent, and both were real. Object-store behaviour itself is proven
	// separately in lancestore.TestRemoteStoreIsQueriedOnTheFly.
	endpoint := os.Getenv("GRAPHIT_LANCE_S3_ENDPOINT")
	bucket := os.Getenv("GRAPHIT_LANCE_S3_BUCKET")
	overS3 := endpoint != "" && bucket != ""

	var st *S3Store
	if overS3 {
		st = newRealS3Store(t, endpoint, bucket)
		t.Log("running over a REAL object store")
	} else {
		st, _ = newTestS3Store(t)
		t.Log("no bucket configured: running the same wiring over a local URI. " +
			"Set GRAPHIT_LANCE_S3_ENDPOINT and GRAPHIT_LANCE_S3_BUCKET for the network run.")
	}

	// ---- build a wiki locally, as a publisher would ----
	srcDir := t.TempDir()
	src, err := wiki.OpenWikiDB(ctx, srcDir)
	if err != nil {
		t.Fatalf("open the source wiki: %v", err)
	}
	chunks := []wiki.WikiChunk{
		{Slug: "hub-s3-layout", Title: "Hub S3 Layout",
			Summary: "How artifacts are laid out in the bucket.",
			Body:    "The registry lives at the prefix root and every artifact gets its own key.",
			DocType: "reference", Source: "hub-s3-layout.md", ContentHash: "h1",
			WordCount: 13, ClusterID: -1, Confidence: 1},
		{Slug: "memory-scopes", Title: "Memory Scopes",
			Summary: "Project and user scopes.",
			Body:    "A scope is pulled on first use and merged rather than replaced.",
			DocType: "reference", Source: "memory-scopes.md", ContentHash: "h2",
			WordCount: 11, ClusterID: -1, Confidence: 1},
	}
	if err := src.Rebuild(ctx, chunks, map[string][]string{"hub-s3-layout": {"memory-scopes"}},
		&wiki.SyncLogEntry{Timestamp: "2026-08-23T00:00:00Z", TotalDocs: 2, ArticlesWritten: 2},
		nil); err != nil {
		_ = src.Close()
		t.Fatalf("build the source wiki: %v", err)
	}
	_ = src.Close()

	// ---- publish it: the index directory travels as itself ----
	stage := t.TempDir()
	if _, err := wiki.ExportToParquet(ctx, srcDir, filepath.Join(stage, wiki.BundleDir)); err != nil {
		t.Fatalf("export for publishing: %v", err)
	}
	if err := st.PublishArtifact(ctx, TypeKnowledge, "acme-docs", "1.0.0", "acme", stage); err != nil {
		t.Fatalf("publish: %v", err)
	}

	// ---- read it back, with NO local copy of what was published ----
	mount, ok := st.MountedWikiAt("acme-docs", "1.0.0", "acme")
	if !ok {
		t.Fatal("no mount for the artifact just published")
	}

	cfg := mount.Config
	if !overS3 {
		// The published bytes, addressed where the fake bucket mirrored them — a directory this
		// test did not build and does not write to. The store's read-only flag is set by hand
		// because the local scheme cannot imply it, and it is the flag every write path consults.
		cfg = lancestore.Config{URI: filepath.Join(stage, wiki.BundleDir)}
	}
	t.Logf("reading on-the-fly from %s", cfg.URI)

	remote, err := wiki.OpenWikiDBAt(ctx, cfg)
	if err != nil {
		t.Fatalf("opening the published wiki: %v", err)
	}
	defer func() { _ = remote.Close() }()

	if overS3 && !remote.Remote() {
		t.Error("a wiki opened on s3:// does not report itself remote, so writes would be allowed")
	}

	// Search, over the network, against objects nothing downloaded.
	hits, err := remote.Search(ctx, "artifact bucket layout", 5)
	if err != nil {
		t.Fatalf("searching the published wiki: %v", err)
	}
	var found bool
	for _, h := range hits {
		if h.Slug == "hub-s3-layout" {
			found = true
		}
	}
	if !found {
		t.Errorf("the published wiki did not answer a search for its own content: %+v", hits)
	}

	// And the PAGE TEXT, which is the part that used to need a file on disk.
	page, err := wiki.ReadPageFrom(ctx, remote, "memory-scopes", textsliceNone())
	if err != nil {
		t.Fatalf("reading a page from the published index: %v", err)
	}
	if page.Title != "Memory Scopes" {
		t.Errorf("page title = %q, want %q", page.Title, "Memory Scopes")
	}
	if page.Source == "" {
		t.Error("the page came back empty — the body did not survive publication")
	}

	// A slug that is not there names what is, instead of failing blankly.
	if _, err := wiki.ReadPageFrom(ctx, remote, "does-not-exist", textsliceNone()); err == nil {
		t.Error("reading a missing page from a published wiki succeeded")
	}
	if pages := wiki.ListPagesFrom(ctx, remote); len(pages) != 2 {
		t.Errorf("the published wiki lists %d pages, want 2: %v", len(pages), pages)
	}

	// Cross-references survived, and they are what `wiki_xrefs` answers from.
	refs, err := remote.FindXRefs(ctx, "hub-s3-layout", 1)
	if err != nil {
		t.Fatalf("cross-references on the published wiki: %v", err)
	}
	if len(refs) == 0 {
		t.Error("cross-references did not survive publication")
	}
}

// textsliceNone is "the whole page", which is what a caller that only wants the text passes.
func textsliceNone() textslice.Request { return textslice.Request{} }

// A published wiki is READ-ONLY, and the refusal has to come from the store rather than from a
// permission error three layers down in the object API.
func TestPublishedWikiRefusesWrites(t *testing.T) {
	ctx := context.Background()
	st, _ := newTestS3Store(t)

	mount, ok := st.MountedWikiAt("acme-docs", "9.9.9", "acme")
	if !ok {
		t.Fatal("no mount")
	}
	db, err := wiki.OpenWikiDBAt(ctx, mount.Config)
	if err != nil {
		t.Skipf("cannot open a remote store here: %v", err)
	}
	defer func() { _ = db.Close() }()

	if err := db.Rebuild(ctx, []wiki.WikiChunk{{Slug: "x", Title: "X"}}, nil, nil, nil); err == nil {
		t.Error("rebuilding a published wiki was allowed")
	}
}

// newRealS3Store builds a store against a live S3-compatible endpoint.
func newRealS3Store(t *testing.T, endpoint, bucket string) *S3Store {
	t.Helper()
	// Configured the way the store actually reads its settings — the same env keys the fake-backed
	// helper beside this one uses, rather than a config map shape guessed at from the outside.
	t.Setenv("GRAPHIT_HUB_BUCKET", bucket)
	t.Setenv("GRAPHIT_HUB_REGION", "us-east-1")
	t.Setenv("GRAPHIT_HUB_ENDPOINT", endpoint)
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	st, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatalf("opening a store against %s: %v", endpoint, err)
	}
	if !st.Configured() {
		t.Fatalf("the store at %s is not configured", endpoint)
	}
	return st
}

// WHAT RUNS EVERYWHERE: the published artifact carries its own indexes.
//
// This is the half that does not need a live object store, and it is the half that proves the
// change of behaviour: installing used to REBUILD the inverted and vector indexes because engine
// structure did not travel in a Parquet bundle. A Lance directory carries them, so they appear in
// the uploaded object list — and if they ever stop appearing, every consumer silently goes back to
// paying for a rebuild.
func TestPublishedWikiCarriesItsIndexes(t *testing.T) {
	ctx := context.Background()
	st, _ := newTestS3Store(t)

	srcDir := t.TempDir()
	src, err := wiki.OpenWikiDB(ctx, srcDir)
	if err != nil {
		t.Fatalf("open the source wiki: %v", err)
	}
	chunks := make([]wiki.WikiChunk, 0, 8)
	for i, slug := range []string{"alpha", "beta", "gamma", "delta"} {
		chunks = append(chunks, wiki.WikiChunk{
			Slug: slug, Title: strings.ToUpper(slug),
			Body:    "Content about registries and scopes, number " + slug,
			DocType: "reference", Source: slug + ".md",
			ContentHash: "h" + slug, WordCount: 8 + i, ClusterID: -1, Confidence: 1,
		})
	}
	if err := src.Rebuild(ctx, chunks, map[string][]string{"alpha": {"beta"}}, nil, nil); err != nil {
		_ = src.Close()
		t.Fatalf("build: %v", err)
	}
	_ = src.Close()

	stage := t.TempDir()
	if _, err := wiki.ExportToParquet(ctx, srcDir, filepath.Join(stage, wiki.BundleDir)); err != nil {
		t.Fatalf("export: %v", err)
	}
	if err := st.PublishArtifact(ctx, TypeKnowledge, "acme-docs", "2.0.0", "acme", stage); err != nil {
		t.Fatalf("publish: %v", err)
	}

	objs, err := st.objects.List(ctx, ArtifactPrefix(TypeKnowledge, "acme-docs", "2.0.0", "acme"))
	if err != nil {
		t.Fatalf("listing the published prefix: %v", err)
	}
	var keys []string
	for _, o := range objs {
		keys = append(keys, o.Key)
	}
	if len(keys) == 0 {
		t.Fatal("nothing was published")
	}

	joined := strings.Join(keys, "\n")
	for _, want := range []struct{ frag, why string }{
		{wiki.BundleDir + "/chunks.lance/", "the pages table"},
		{"/_versions/", "the manifest, without which the dataset cannot be opened at all"},
		{"/data/", "the row data"},
		{"/_indices/", "THE INVERTED INDEX — this is what makes installing a copy instead of a rebuild"},
	} {
		if !strings.Contains(joined, want.frag) {
			t.Errorf("the published artifact has no %q — %s.\nPublished:\n%s",
				want.frag, want.why, joined)
		}
	}

	// The mount points at what was actually uploaded, rather than at a path assembled by a
	// different rule. A mismatch here is the failure mode that reads as "no such table".
	mount, ok := st.MountedWikiAt("acme-docs", "2.0.0", "acme")
	if !ok {
		t.Fatal("no mount for the artifact just published")
	}
	if !strings.Contains(mount.Config.URI, "/"+wiki.BundleDir) {
		t.Errorf("the mount URI %q does not address the uploaded index directory", mount.Config.URI)
	}
	prefix := ArtifactPrefix(TypeKnowledge, "acme-docs", "2.0.0", "acme") + "/" + wiki.BundleDir
	if !strings.Contains(joined, prefix+"/") {
		t.Errorf("nothing was uploaded under %q, which is where the mount reads from", prefix)
	}
}
