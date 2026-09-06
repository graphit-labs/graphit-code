package hub

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

func newTestS3Store(t *testing.T) (*S3Store, *testsupport.FakeS3) {
	t.Helper()
	fake, endpoint := testsupport.StartFakeS3(t, "graphit-hub")
	t.Setenv("GRAPHIT_HUB_BUCKET", "graphit-hub")
	t.Setenv("GRAPHIT_HUB_REGION", "us-east-1")
	t.Setenv("GRAPHIT_HUB_ENDPOINT", endpoint)
	t.Setenv("GRAPHIT_HUB_ACCESS_KEY_ID", "test")
	t.Setenv("GRAPHIT_HUB_SECRET_ACCESS_KEY", "test")
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatalf("NewS3Store: %v", err)
	}
	return store, fake
}

func TestS3StoreReadsAndWritesOnlyFullyQualifiedV2Keys(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := context.Background()
	key := hubaccess.ProjectMetadataKey(testProjectOne)
	if err := store.WriteFile(ctx, key, []byte(`{"v":2}`)); err != nil {
		t.Fatal(err)
	}
	if keys := fake.Keys(); len(keys) != 1 || keys[0] != key {
		t.Fatalf("bucket keys = %v", keys)
	}
	if _, err := store.ReadFile(ctx, key); err != nil {
		t.Fatal(err)
	}
}

func TestArtifactPrefixIsProjectScopedForEveryType(t *testing.T) {
	for _, artifactType := range ValidTypes {
		prefix := ArtifactPrefix(artifactType, "shared-id", "branch/feature/api", testProjectOne)
		wantStart := "v2/projects/" + testProjectOne + "/artifacts/" + TypeFolderMap[artifactType] + "/shared-id/"
		if !strings.HasPrefix(prefix, wantStart) {
			t.Fatalf("ArtifactPrefix(%s) = %q", artifactType, prefix)
		}
		if strings.Contains(strings.TrimPrefix(prefix, wantStart), "/") {
			t.Fatalf("version created nested prefix: %q", prefix)
		}
	}
	if got := ArtifactPrefix(TypeSkill, "skill", "1", ""); got != "" {
		t.Fatalf("artifact without project ULID = %q", got)
	}
	store, _ := newTestS3Store(t)
	if got := store.ArtifactURI(TypeSkill, "skill", "1", "", "payload"); got != "" {
		t.Fatalf("artifact without project ULID produced URI %q", got)
	}
}

func TestAuthorizedPublishAndDeleteStayInsideProjectPrefix(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	if _, err := registryForStore(ctx, store).UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	source := t.TempDir()
	if err := os.WriteFile(filepath.Join(source, "SKILL.md"), []byte("body"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := store.PublishArtifact(ctx, TypeSkill, "lint", "1.0.0", testProjectOne, source); err != nil {
		t.Fatal(err)
	}
	prefix := hubaccess.ProjectArtifactPrefix(testProjectOne, "skills", "lint", "1.0.0")
	if _, ok := fake.Object(prefix + "/SKILL.md"); !ok {
		t.Fatalf("published keys = %v", fake.Keys())
	}
	if err := store.DeleteArtifact(ctx, TypeSkill, "lint", "1.0.0", testProjectOne); err != nil {
		t.Fatal(err)
	}
	if _, ok := fake.Object(prefix + "/SKILL.md"); ok {
		t.Fatal("artifact survived delete")
	}
}

func TestArtifactReadFailsClosedAfterGrantRevocation(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	if _, err := registryForStore(ctx, store).UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	allowProjects(t, ctx, store)
	if _, err := store.DownloadArtifact(ctx, TypeSkill, "lint", "1.0.0", testProjectOne); !errors.Is(err, hubaccess.ErrDenied) {
		t.Fatalf("revoked download error = %v", err)
	}
}

func TestRulesAndEventsUseV2Namespaces(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	if _, err := registryForStore(ctx, store).UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	if err := store.WriteRule(ctx, "ast.md", []byte("rule")); err != nil {
		t.Fatal(err)
	}
	key := EventKey(testProjectOne, "ast", "artifact.install", time.Now(), "01ABC")
	store.WriteEventFile(ctx, testProjectOne, key, []byte("{}"))
	WaitForPendingEvents()
	if _, ok := fake.Object("v2/global/rules/ast.md"); !ok {
		t.Fatalf("rule missing from v2 namespace: %v", fake.Keys())
	}
	if _, ok := fake.Object(key); !ok {
		t.Fatalf("event missing from project namespace: %v", fake.Keys())
	}
}

func TestEventKeyRejectsUnsafeSegments(t *testing.T) {
	for _, tc := range []struct {
		artifactType string
		action       string
		unique       string
	}{
		{artifactType: "../ast", action: "artifact.install", unique: "01ABC"},
		{artifactType: "ast", action: "artifact/install", unique: "01ABC"},
		{artifactType: "ast", action: "artifact.install", unique: "../01ABC"},
	} {
		if got := EventKey(testProjectOne, tc.artifactType, tc.action, time.Now(), tc.unique); got != "" {
			t.Fatalf("unsafe event key = %q", got)
		}
	}
}

func TestEventUploadRevalidatesRevokedProject(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	if _, err := registryForStore(ctx, store).UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	allowProjects(t, ctx, store)
	key := EventKey(testProjectOne, "ast", "artifact.install", time.Now(), "01ABC")
	store.WriteEventFile(ctx, testProjectOne, key, []byte("{}"))
	WaitForPendingEvents()
	if _, ok := fake.Object(key); ok {
		t.Fatal("revoked project wrote an event")
	}
}

func TestRemoteOperationsInLocalOnlyModeReturnErrNotConfigured(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := store.ReadFile(context.Background(), hubaccess.BaselinesKey()); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Fatalf("ReadFile error = %v", err)
	}
}
