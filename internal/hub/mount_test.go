package hub

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func TestMountedWikiURIPointsAtThePublishedIndex(t *testing.T) {
	st, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, st, hubaccess.Selector{All: true})
	if _, err := registryForStore(ctx, st).UpsertProject(ctx, testProjectOne, "acme", ""); err != nil {
		t.Fatal(err)
	}

	mount, ok, err := st.MountedWikiAt(ctx, "acme-docs", "1.4.0", testProjectOne)
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("a configured store did not produce a mount")
	}

	if !strings.HasSuffix(mount.Config.URI, "/"+wiki.WikiIndexDirName) {
		t.Errorf("the mount URI does not end at the index directory %q: %s",
			wiki.WikiIndexDirName, mount.Config.URI)
	}
	if !strings.HasPrefix(mount.Config.URI, "s3://") {
		t.Errorf("the mount URI is not an s3:// location: %s", mount.Config.URI)
	}
	if !strings.Contains(mount.Config.URI, "1.4.0") {
		t.Errorf("the mount URI does not carry the version: %s", mount.Config.URI)
	}
	if mount.Config.S3.Bucket == "" {
		t.Error("the mount carries no bucket configuration, so the engine cannot reach it")
	}
}

// The URI is DERIVED from the record, never stored, so it has to be reproducible. If it were not,
// nothing would notice until a read failed against a location that had drifted.
func TestMountedWikiURIIsStable(t *testing.T) {
	st, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, st, hubaccess.Selector{All: true})
	if _, err := registryForStore(ctx, st).UpsertProject(ctx, testProjectOne, "acme", ""); err != nil {
		t.Fatal(err)
	}

	first, ok1, err1 := st.MountedWikiAt(ctx, "acme-docs", "1.4.0", testProjectOne)
	second, ok2, err2 := st.MountedWikiAt(ctx, "acme-docs", "1.4.0", testProjectOne)
	if err1 != nil || err2 != nil {
		t.Fatalf("mount errors: %v, %v", err1, err2)
	}
	if !ok1 || !ok2 {
		t.Fatal("the mount did not resolve")
	}
	if first.Config.URI != second.Config.URI {
		t.Errorf("the derived URI is not stable: %q then %q", first.Config.URI, second.Config.URI)
	}
}

// A version is not optional. An artifact prefix without one is the shared root, and reading a
// dataset from there would either fail or — worse — succeed against whatever else lives under it.
func TestMountedWikiRefusesAnEmptyVersion(t *testing.T) {
	st, _ := newTestS3Store(t)

	if _, ok, _ := st.MountedWikiAt(context.Background(), "acme-docs", "", testProjectOne); ok {
		t.Error("a mount resolved for an artifact with no version")
	}
}

// An unconfigured store cannot mount anything, and saying otherwise would leave an install with no
// bytes transferred and no location to read them from.
func TestMountedWikiRefusesWhenTheHubIsNotConfigured(t *testing.T) {
	var st *S3Store
	if _, ok, _ := st.MountedWikiAt(context.Background(), "acme-docs", "1.4.0", testProjectOne); ok {
		t.Error("a nil store produced a mount")
	}

	empty := &S3Store{}
	if empty.Configured() {
		t.Fatal("an empty store reports itself configured; this test proves nothing")
	}
	if _, ok, _ := empty.MountedWikiAt(context.Background(), "acme-docs", "1.4.0", testProjectOne); ok {
		t.Error("an unconfigured store produced a mount")
	}
}

// MountsKnowledge is the gate the install path consults, and it has to answer for BOTH conditions:
// a bucket to read from and an engine that can read it. A build without the search engine linked
// in must keep downloading — answering yes there installs a context with no bytes and no way to
// open them.
func TestMountsKnowledgeNeedsBothTheBucketAndTheEngine(t *testing.T) {
	st, _ := newTestS3Store(t)

	withStore := &RegistryManager{store: st}
	withoutStore := &RegistryManager{}

	if withoutStore.MountsKnowledge() {
		t.Error("a manager with no store says it can mount")
	}

	got := withStore.MountsKnowledge()
	if got != lancestoreAvailableForTest() {
		t.Errorf("MountsKnowledge = %v with a configured store, but the engine's availability "+
			"is %v — the two must agree", got, lancestoreAvailableForTest())
	}
}

func lancestoreAvailableForTest() bool { return lancestore.Available() }
