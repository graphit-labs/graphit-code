package s3store

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

func newTestStore(t *testing.T, prefix string) (*Store, *testsupport.FakeS3) {
	t.Helper()

	fake, endpoint := testsupport.StartFakeS3(t, "graphit-hub")

	store, err := New(context.Background(), config.S3Config{
		Bucket:   "graphit-hub",
		Region:   "us-east-1",
		Endpoint: endpoint,
		Prefix:   prefix,
	})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	return store, fake
}

func TestNewWithoutBucketIsNotConfigured(t *testing.T) {
	_, err := New(context.Background(), config.S3Config{})
	if !errors.Is(err, ErrNotConfigured) {
		t.Fatalf("want ErrNotConfigured, got %v", err)
	}
}

func TestConfiguredCredentialsOverrideTheAWSDefaultChain(t *testing.T) {
	fake, endpoint := testsupport.StartFakeS3(t, "graphit-hub")
	t.Setenv("AWS_ACCESS_KEY_ID", "environment-key")
	t.Setenv("AWS_SECRET_ACCESS_KEY", "environment-secret")

	store, err := New(context.Background(), config.S3Config{
		Bucket:          "graphit-hub",
		Region:          "us-east-1",
		Endpoint:        endpoint,
		AccessKeyID:     "configured-key",
		SecretAccessKey: "configured-secret",
	})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	if err := store.EnsureBucket(context.Background()); err != nil {
		t.Fatalf("EnsureBucket: %v", err)
	}
	if got := fake.LastAccessKey(); got != "configured-key" {
		t.Fatalf("signed access key = %q; want configured-key", got)
	}
}

func TestPutGetExistsDelete(t *testing.T) {
	store, _ := newTestStore(t, "")
	ctx := context.Background()

	if err := store.Put(ctx, "registry.json", []byte(`{"v":1}`)); err != nil {
		t.Fatalf("Put: %v", err)
	}

	got, err := store.Get(ctx, "registry.json")
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if string(got) != `{"v":1}` {
		t.Fatalf("Get returned %q", got)
	}

	exists, err := store.Exists(ctx, "registry.json")
	if err != nil || !exists {
		t.Fatalf("Exists = %v, %v; want true, nil", exists, err)
	}

	if err := store.Delete(ctx, "registry.json"); err != nil {
		t.Fatalf("Delete: %v", err)
	}
	exists, err = store.Exists(ctx, "registry.json")
	if err != nil || exists {
		t.Fatalf("Exists after delete = %v, %v; want false, nil", exists, err)
	}
}

// A missing object and a broken bucket are different problems: the first is a first run,
// the second is a misconfiguration, and the callers branch on exactly this.
func TestGetMissingObjectIsErrNotFound(t *testing.T) {
	store, _ := newTestStore(t, "")

	_, err := store.Get(context.Background(), "absent.json")
	if !errors.Is(err, ErrNotFound) {
		t.Fatalf("want ErrNotFound, got %v", err)
	}
}

func TestPrefixIsAppliedAndHiddenFromCallers(t *testing.T) {
	store, fake := newTestStore(t, "team-a")
	ctx := context.Background()

	if err := store.Put(ctx, "artifacts/ast/demo/1.0.0/schema.cypher", []byte("CREATE")); err != nil {
		t.Fatalf("Put: %v", err)
	}

	if _, ok := fake.Object("team-a/artifacts/ast/demo/1.0.0/schema.cypher"); !ok {
		t.Fatalf("prefix not applied; stored keys: %v", fake.Keys())
	}

	objs, err := store.List(ctx, "artifacts/ast/demo")
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(objs) != 1 || objs[0].Key != "artifacts/ast/demo/1.0.0/schema.cypher" {
		t.Fatalf("List returned %+v; want the prefix stripped", objs)
	}
}

func TestListReturnsSizesAndOnlyMatchingPrefix(t *testing.T) {
	store, _ := newTestStore(t, "")
	ctx := context.Background()

	for key, body := range map[string]string{
		"artifacts/ast/a/1.0.0/nodes.parquet": "abc",
		"artifacts/ast/b/1.0.0/nodes.parquet": "de",
		"events/2026/08/one.ndjson":           "f",
	} {
		if err := store.Put(ctx, key, []byte(body)); err != nil {
			t.Fatalf("Put %s: %v", key, err)
		}
	}

	objs, err := store.List(ctx, "artifacts/ast")
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(objs) != 2 {
		t.Fatalf("List returned %d objects, want 2: %+v", len(objs), objs)
	}
	if objs[0].Size != 3 {
		t.Fatalf("first object size = %d, want 3", objs[0].Size)
	}
}

func TestDeletePrefixRemovesTheWholeVersion(t *testing.T) {
	store, fake := newTestStore(t, "")
	ctx := context.Background()

	for _, key := range []string{
		"artifacts/ast/demo/1.0.0/nodes_File.parquet",
		"artifacts/ast/demo/1.0.0/indptr_File.parquet",
		"artifacts/ast/demo/1.0.0/schema.cypher",
		"artifacts/ast/demo/2.0.0/schema.cypher",
	} {
		if err := store.Put(ctx, key, []byte("x")); err != nil {
			t.Fatalf("Put %s: %v", key, err)
		}
	}

	if err := store.DeletePrefix(ctx, "artifacts/ast/demo/1.0.0"); err != nil {
		t.Fatalf("DeletePrefix: %v", err)
	}

	remaining := fake.Keys()
	if len(remaining) != 1 || remaining[0] != "artifacts/ast/demo/2.0.0/schema.cypher" {
		t.Fatalf("remaining keys = %v; want only version 2.0.0", remaining)
	}
}

func TestDeletePrefixOnEmptyPrefixIsNotAnError(t *testing.T) {
	store, _ := newTestStore(t, "")

	if err := store.DeletePrefix(context.Background(), "artifacts/ast/never-published"); err != nil {
		t.Fatalf("DeletePrefix: %v", err)
	}
}

func TestUploadDirAndDownloadPrefixRoundTrip(t *testing.T) {
	store, _ := newTestStore(t, "hub")
	ctx := context.Background()

	src := t.TempDir()
	if err := os.MkdirAll(filepath.Join(src, "graph"), 0o755); err != nil {
		t.Fatal(err)
	}
	files := map[string]string{
		"schema.cypher":             "CREATE NODE TABLE File(...)",
		"graph/nodes_File.parquet":  "PAR1-nodes",
		"graph/indptr_File.parquet": "PAR1-indptr",
	}
	for rel, body := range files {
		if err := os.WriteFile(filepath.Join(src, filepath.FromSlash(rel)), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	if err := store.UploadDir(ctx, src, "artifacts/ast/demo/1.0.0"); err != nil {
		t.Fatalf("UploadDir: %v", err)
	}

	dst := t.TempDir()
	if err := store.DownloadPrefix(ctx, "artifacts/ast/demo/1.0.0", dst); err != nil {
		t.Fatalf("DownloadPrefix: %v", err)
	}

	for rel, want := range files {
		got, err := os.ReadFile(filepath.Join(dst, filepath.FromSlash(rel)))
		if err != nil {
			t.Fatalf("reading %s back: %v", rel, err)
		}
		if string(got) != want {
			t.Fatalf("%s = %q, want %q", rel, got, want)
		}
	}
}

func TestUploadDirRemovesObjectsMissingFromTheLocalDirectory(t *testing.T) {
	store, fake := newTestStore(t, "")
	ctx := context.Background()
	prefix := "artifacts/knowledge/demo/branch-main"

	if err := store.Put(ctx, prefix+"/index.lance/data/obsolete.lance", []byte("old")); err != nil {
		t.Fatalf("seed stale object: %v", err)
	}
	if err := store.Put(ctx, prefix+"/index.lance/_versions/1.manifest", []byte("old-manifest")); err != nil {
		t.Fatalf("seed replaced object: %v", err)
	}

	src := t.TempDir()
	manifest := filepath.Join(src, "index.lance", "_versions", "1.manifest")
	if err := os.MkdirAll(filepath.Dir(manifest), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(manifest, []byte("new-manifest"), 0o644); err != nil {
		t.Fatal(err)
	}
	data := filepath.Join(src, "index.lance", "data", "current.lance")
	if err := os.MkdirAll(filepath.Dir(data), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(data, []byte("new"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := store.UploadDir(ctx, src, prefix); err != nil {
		t.Fatalf("UploadDir: %v", err)
	}

	want := []string{
		prefix + "/index.lance/_versions/1.manifest",
		prefix + "/index.lance/data/current.lance",
	}
	if got := fake.Keys(); !slices.Equal(got, want) {
		t.Fatalf("keys = %v, want %v", got, want)
	}
	if got, ok := fake.Object(prefix + "/index.lance/_versions/1.manifest"); !ok || string(got) != "new-manifest" {
		t.Fatalf("manifest = %q, present=%v", got, ok)
	}
}

func TestEnsureBucketFailsOnAnUnknownBucket(t *testing.T) {
	_, endpoint := testsupport.StartFakeS3(t, "graphit-hub")

	store, err := New(context.Background(), config.S3Config{
		Bucket:   "not-the-bucket",
		Region:   "us-east-1",
		Endpoint: endpoint,
	})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	if err := store.EnsureBucket(context.Background()); err == nil {
		t.Fatal("EnsureBucket succeeded against a bucket the server does not have")
	}
}

func TestEnsureBucketSucceeds(t *testing.T) {
	store, _ := newTestStore(t, "")

	if err := store.EnsureBucket(context.Background()); err != nil {
		t.Fatalf("EnsureBucket: %v", err)
	}
}

// The URI is what LadybugDB receives as a table's `storage` and what LanceDB receives as a
// connection target, so its exact shape is load-bearing rather than cosmetic.
func TestURIIsTheFormBothEnginesMount(t *testing.T) {
	store, _ := newTestStore(t, "team-a")

	got := store.URI("artifacts/ast/demo/1.0.0")
	want := "s3://graphit-hub/team-a/artifacts/ast/demo/1.0.0"
	if got != want {
		t.Fatalf("URI = %q, want %q", got, want)
	}
}

func TestJoinKeyCollapsesEmptyAndSlashHeavyParts(t *testing.T) {
	cases := []struct {
		parts []string
		want  string
	}{
		{[]string{"", "artifacts", "", "ast"}, "artifacts/ast"},
		{[]string{"/team-a/", "/artifacts/"}, "team-a/artifacts"},
		{[]string{" ", "registry.json"}, "registry.json"},
		{nil, ""},
	}
	for _, c := range cases {
		if got := JoinKey(c.parts...); got != c.want {
			t.Errorf("JoinKey(%q) = %q, want %q", c.parts, got, c.want)
		}
	}
}

func TestURIWithoutBucketIsEmpty(t *testing.T) {
	if got := URI("", "some/key"); got != "" {
		t.Fatalf("URI with no bucket = %q, want empty", got)
	}
	if got := URI("b", ""); got != "s3://b" {
		t.Fatalf("URI with no key = %q, want s3://b", got)
	}
}
