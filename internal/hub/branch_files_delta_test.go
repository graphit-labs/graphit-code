package hub

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

func TestPublishBranchFilesUploadsOnlyChangedObjects(t *testing.T) {
	ctx := trustedHubContext(t)
	fake, endpoint := testsupport.StartFakeS3(t, "branch-delta")
	cfg := config.S3Config{Bucket: "branch-delta", Region: "us-east-1", Endpoint: endpoint,
		AccessKeyID: "test-key", SecretAccessKey: "test-secret"}
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	remote := &S3Store{objects: objects, cfg: cfg}
	allowProjects(t, ctx, remote, hubaccess.Selector{All: true})
	registry := registryForStore(ctx, remote)
	if _, err := registry.UpsertProject(ctx, testProjectOne, "graphit-code", ""); err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	a := filepath.Join(dir, "a.md")
	b := filepath.Join(dir, "b.md")
	if err := os.WriteFile(a, []byte("first"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(b, []byte("unchanged"), 0o644); err != nil {
		t.Fatal(err)
	}
	publish := func() {
		t.Helper()
		if err := remote.PublishBranchFiles(ctx, TypeKnowledge, "docs", "branch/main", testProjectOne, dir); err != nil {
			t.Fatal(err)
		}
	}
	keyA := ArtifactPrefix(TypeKnowledge, "docs", "branch/main", testProjectOne) + "/a.md"
	keyB := ArtifactPrefix(TypeKnowledge, "docs", "branch/main", testProjectOne) + "/b.md"
	publish()
	if fake.RequestCount("PUT", keyA) != 1 || fake.RequestCount("PUT", keyB) != 1 {
		t.Fatal("initial branch files were not uploaded exactly once")
	}
	publish()
	if fake.RequestCount("PUT", keyA) != 1 || fake.RequestCount("PUT", keyB) != 1 {
		t.Fatal("unchanged branch files were uploaded again")
	}
	if err := os.WriteFile(a, []byte("second"), 0o644); err != nil {
		t.Fatal(err)
	}
	publish()
	if fake.RequestCount("PUT", keyA) != 2 || fake.RequestCount("PUT", keyB) != 1 {
		t.Fatal("branch upload did not isolate the changed file")
	}
	if err := os.Remove(a); err != nil {
		t.Fatal(err)
	}
	publish()
	if _, ok := fake.Object(keyA); ok || fake.RequestCount("PUT", keyB) != 1 {
		t.Fatal("branch deletion changed an unrelated file")
	}
}
