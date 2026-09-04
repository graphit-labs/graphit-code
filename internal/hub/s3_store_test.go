package hub

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

func newTestS3Store(t *testing.T) (*S3Store, *testsupport.FakeS3) {
	t.Helper()

	fake, endpoint := testsupport.StartFakeS3(t, "graphit-hub")
	t.Setenv("GRAPHIT_HUB_BUCKET", "graphit-hub")
	t.Setenv("GRAPHIT_HUB_REGION", "us-east-1")
	t.Setenv("GRAPHIT_HUB_ENDPOINT", endpoint)
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatalf("NewS3Store: %v", err)
	}
	if !store.Configured() {
		t.Fatal("store reports itself unconfigured with a bucket set")
	}
	return store, fake
}

func newFakeBackedStore(t *testing.T, dir string) (*S3Store, *testsupport.FakeS3) {
	t.Helper()
	st, fake := newTestS3Store(t)
	st.cacheBase = dir
	return st, fake
}

func TestNewS3StoreWithoutABucketIsLocalOnlyNotAnError(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")

	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatalf("NewS3Store without a bucket returned an error: %v", err)
	}
	if store.Configured() {
		t.Fatal("store reports itself configured with no bucket")
	}
	if err := store.EnsureReachable(context.Background()); err != nil {
		t.Fatalf("EnsureReachable in local-only mode: %v", err)
	}
}

func TestRegistryDocumentRoundTrip(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := context.Background()

	rel := "projects/ab/abcdef/ast_2.1.0.json"
	if err := store.WriteFile(ctx, rel, []byte(`{"v":1}`)); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	if _, ok := fake.Object("registry/projects/ab/abcdef/ast_2.1.0.json"); !ok {
		t.Fatalf("registry document not under the registry prefix; keys: %v", fake.Keys())
	}

	got, err := store.ReadFile(ctx, rel)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(got) != `{"v":1}` {
		t.Fatalf("ReadFile = %q", got)
	}

	if err := store.RemoveFile(ctx, rel); err != nil {
		t.Fatalf("RemoveFile: %v", err)
	}
	if _, err := store.ReadFile(ctx, rel); !errors.Is(err, s3store.ErrNotFound) {
		t.Fatalf("after RemoveFile, want ErrNotFound, got %v", err)
	}
}

func TestRegistryRevisionChangesAfterSameSizeRewrite(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := context.Background()
	rel := "projects/ab/abcdef/ast_branch-main.json"
	if err := store.WriteFile(ctx, rel, []byte(`{"v":1,"entry":"a"}`)); err != nil {
		t.Fatal(err)
	}
	before := store.RegistryRevision(ctx)
	if err := store.WriteFile(ctx, rel, []byte(`{"v":1,"entry":"b"}`)); err != nil {
		t.Fatal(err)
	}
	after := store.RegistryRevision(ctx)
	if before == "" || after == "" || before == after {
		t.Fatalf("registry revision did not detect same-size rewrite: before=%q after=%q", before, after)
	}
}

// The registry's own layout is two levels of hash fan-out, so listing must reach entries
// below the immediate children.
func TestListDirWalksBelowTheImmediateChildren(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := context.Background()

	for _, rel := range []string{
		"projects/ab/abcdef/project.json",
		"projects/ab/abcdef/skill_testing_1.0.0.json",
		"projects/cd/cdefgh/project.json",
	} {
		if err := store.WriteFile(ctx, rel, []byte("{}")); err != nil {
			t.Fatalf("WriteFile %s: %v", rel, err)
		}
	}

	got, err := store.ListDir(ctx, "projects/ab")
	if err != nil {
		t.Fatalf("ListDir: %v", err)
	}
	want := []string{"abcdef/project.json", "abcdef/skill_testing_1.0.0.json"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("ListDir = %v, want %v", got, want)
	}
}

// The prefix layout is a contract with docs/specs/hub-s3-object-layout.md, including the rule
// that ast and knowledge carry no id segment.
func TestArtifactPrefixLayout(t *testing.T) {
	cases := []struct {
		artType           ArtifactType
		id, version, proj string
		want              string
	}{
		{TypeSkill, "error-handling", "1.2.0", "payments", "artifacts/skills/payments/error-handling/1.2.0"},
		{TypeSkill, "error-handling", "1.2.0", "", "artifacts/skills/_global/error-handling/1.2.0"},
		{TypeAST, "payments-core", "2.1.0", "payments", "artifacts/ast/payments/2.1.0"},
		{TypeKnowledge, "ignored", "3.0.0", "payments", "artifacts/knowledge/payments/3.0.0"},
		{TypeMCP, "stripe", "1.0.0", "", "artifacts/mcp-servers/_global/stripe/1.0.0"},
	}
	for _, c := range cases {
		if got := ArtifactPrefix(c.artType, c.id, c.version, c.proj); got != c.want {
			t.Errorf("ArtifactPrefix(%s, %s, %s, %q) = %q, want %q", c.artType, c.id, c.version, c.proj, got, c.want)
		}
	}
}

func TestArtifactPrefixEncodesNamedVersionAsOneSegment(t *testing.T) {
	t.Parallel()
	got := ArtifactPrefix(TypeAST, "payments", "branch/feature/hub-sync", "project-1")
	want := "artifacts/ast/project-1/~YnJhbmNoL2ZlYXR1cmUvaHViLXN5bmM"
	if got != want {
		t.Fatalf("ArtifactPrefix() = %q, want %q", got, want)
	}
}

// This URI is what a graph table receives as `storage` and what the search index receives as
// its connection target, so its exact shape is load-bearing.
func TestArtifactURIIsWhatTheEnginesMount(t *testing.T) {
	fake, endpoint := testsupport.StartFakeS3(t, "graphit-hub")
	_ = fake
	t.Setenv("GRAPHIT_HUB_BUCKET", "graphit-hub")
	t.Setenv("GRAPHIT_HUB_REGION", "us-east-1")
	t.Setenv("GRAPHIT_HUB_ENDPOINT", endpoint)
	t.Setenv("GRAPHIT_HUB_PREFIX", "team-a")

	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatalf("NewS3Store: %v", err)
	}

	got := store.ArtifactURI(TypeAST, "payments-core", "2.1.0", "payments", "graph")
	want := "s3://graphit-hub/team-a/artifacts/ast/payments/2.1.0/graph"
	if got != want {
		t.Fatalf("ArtifactURI = %q, want %q", got, want)
	}
}

func TestPublishAndDeleteArtifactMoveTheWholePrefix(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := context.Background()

	src := t.TempDir()
	if err := os.MkdirAll(filepath.Join(src, "graph"), 0o755); err != nil {
		t.Fatal(err)
	}
	for rel, body := range map[string]string{
		"schema.cypher":            "CREATE NODE TABLE File(...)",
		"graph/nodes_File.parquet": "PAR1",
	} {
		if err := os.WriteFile(filepath.Join(src, filepath.FromSlash(rel)), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	if err := store.PublishArtifact(ctx, TypeAST, "payments-core", "2.1.0", "payments", src); err != nil {
		t.Fatalf("PublishArtifact: %v", err)
	}
	if _, ok := fake.Object("artifacts/ast/payments/2.1.0/graph/nodes_File.parquet"); !ok {
		t.Fatalf("artifact not published to its prefix; keys: %v", fake.Keys())
	}

	if err := store.DeleteArtifact(ctx, TypeAST, "payments-core", "2.1.0", "payments"); err != nil {
		t.Fatalf("DeleteArtifact: %v", err)
	}
	if keys := fake.Keys(); len(keys) != 0 {
		t.Fatalf("DeleteArtifact left %v", keys)
	}
}

// Deleting one version must not touch its siblings: the prefix is the unit, and the version
// is the last segment of it.
func TestDeleteArtifactLeavesOtherVersions(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := context.Background()

	src := t.TempDir()
	if err := os.WriteFile(filepath.Join(src, "schema.cypher"), []byte("CREATE"), 0o644); err != nil {
		t.Fatal(err)
	}
	for _, v := range []string{"2.0.0", "2.1.0"} {
		if err := store.PublishArtifact(ctx, TypeAST, "payments-core", v, "payments", src); err != nil {
			t.Fatalf("PublishArtifact %s: %v", v, err)
		}
	}

	if err := store.DeleteArtifact(ctx, TypeAST, "payments-core", "2.0.0", "payments"); err != nil {
		t.Fatalf("DeleteArtifact: %v", err)
	}

	keys := fake.Keys()
	if len(keys) != 1 || keys[0] != "artifacts/ast/payments/2.1.0/schema.cypher" {
		t.Fatalf("keys = %v; want only 2.1.0", keys)
	}
}

func TestEnsureArtifactLocalDownloadsAFileBasedArtifact(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := context.Background()
	store.cacheBase = t.TempDir()

	src := t.TempDir()
	if err := os.WriteFile(filepath.Join(src, "SKILL.md"), []byte("# Error handling"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := store.PublishArtifact(ctx, TypeSkill, "error-handling", "1.0.0", "", src); err != nil {
		t.Fatalf("PublishArtifact: %v", err)
	}

	dir, err := store.EnsureArtifactLocal(ctx, TypeSkill, "error-handling", "1.0.0", "")
	if err != nil {
		t.Fatalf("EnsureArtifactLocal: %v", err)
	}
	got, err := os.ReadFile(filepath.Join(dir, "SKILL.md"))
	if err != nil {
		t.Fatalf("reading the downloaded artifact: %v", err)
	}
	if string(got) != "# Error handling" {
		t.Fatalf("downloaded content = %q", got)
	}
}

// The whole point of the migration: a mountable artifact is read in place. Downloading one
// here would silently reintroduce the transfer this removed, so the method refuses instead.
func TestEnsureArtifactLocalRefusesAMountableType(t *testing.T) {
	store, _ := newTestS3Store(t)
	store.cacheBase = t.TempDir()

	_, err := store.EnsureArtifactLocal(context.Background(), TypeAST, "payments-core", "2.1.0", "payments")
	if err == nil {
		t.Fatal("EnsureArtifactLocal downloaded a mountable artifact")
	}
	if !strings.Contains(err.Error(), "ArtifactURI") {
		t.Fatalf("error should point at the mounting route: %v", err)
	}
}

func TestEnsureArtifactLocalOnAnUnpublishedVersionIsNotFound(t *testing.T) {
	store, _ := newTestS3Store(t)
	store.cacheBase = t.TempDir()

	_, err := store.EnsureArtifactLocal(context.Background(), TypeSkill, "absent", "9.9.9", "")
	if !errors.Is(err, s3store.ErrNotFound) {
		t.Fatalf("want ErrNotFound, got %v", err)
	}
}

func TestEnsureArtifactLocalRefreshesAMutablePublishedVersion(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := context.Background()
	store.cacheBase = t.TempDir()

	src := t.TempDir()
	if err := os.WriteFile(filepath.Join(src, "SKILL.md"), []byte("first"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := store.PublishArtifact(ctx, TypeSkill, "s", "1.0.0", "", src); err != nil {
		t.Fatal(err)
	}
	if _, err := store.EnsureArtifactLocal(ctx, TypeSkill, "s", "1.0.0", ""); err != nil {
		t.Fatal(err)
	}
	cacheDir := store.ArtifactCacheDir(TypeSkill, "s", "1.0.0", "")
	if err := os.WriteFile(filepath.Join(cacheDir, "obsolete.md"), []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}

	fake.Put("artifacts/skills/_global/s/1.0.0/SKILL.md", []byte("second"))

	dir, err := store.EnsureArtifactLocal(ctx, TypeSkill, "s", "1.0.0", "")
	if err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(filepath.Join(dir, "SKILL.md"))
	if string(got) != "second" {
		t.Fatalf("cache content = %q, want refreshed content", got)
	}
	if _, err := os.Stat(filepath.Join(dir, "obsolete.md")); !os.IsNotExist(err) {
		t.Fatalf("obsolete cached file survived refresh: %v", err)
	}
}

func TestEventsUploadDirectlyAndDoNotAccumulate(t *testing.T) {
	store, fake := newTestS3Store(t)
	store.cacheBase = t.TempDir()

	key := EventKey("payments", "ast", "artifact.install", time.Date(2026, 8, 21, 15, 4, 5, 0, time.UTC), "01ABC")
	store.WriteEventFile(key, []byte(`{"action":"artifact.install"}`))
	WaitForPendingEvents()

	keys := fake.Keys()
	if len(keys) != 1 || !strings.HasPrefix(keys[0], "events/payments/ast/") {
		t.Fatalf("event not uploaded under the events prefix: %v", keys)
	}

	if entries, err := os.ReadDir(filepath.Join(store.cacheBase, eventsStagingSubdir)); err == nil && len(entries) != 0 {
		t.Errorf("a successful event was staged anyway: %v", entries)
	}
}

// The key must survive a retry intact. The old code staged under the key with "/" replaced by "_"
// and rebuilt it with the inverse replacement — but a key already contains underscores, in the ULID
// and in the action, so every retried event landed under a mangled key.
func TestAFailedEventRetriesUnderTheSameKey(t *testing.T) {
	store, fake := newTestS3Store(t)
	store.cacheBase = t.TempDir()

	key := EventKey("payments", "ast", "artifact.install", time.Date(2026, 8, 21, 15, 4, 5, 0, time.UTC), "01ABC")
	wantKey := "events/" + key

	store.stageEvent(wantKey, []byte(`{"action":"artifact.install"}`))
	staged, err := os.ReadDir(filepath.Join(store.cacheBase, eventsStagingSubdir))
	if err != nil || len(staged) != 1 {
		t.Fatalf("stageEvent did not stage: %v, %v", staged, err)
	}

	store.SyncEvents(context.Background())

	if data, ok := fake.Object(wantKey); !ok {
		t.Errorf("retried event did not land under %q; bucket holds %v", wantKey, fake.Keys())
	} else if string(data) != `{"action":"artifact.install"}` {
		t.Errorf("retried body = %q", data)
	}
	if remaining, _ := os.ReadDir(filepath.Join(store.cacheBase, eventsStagingSubdir)); len(remaining) != 0 {
		t.Errorf("a retried event stayed staged: %v", remaining)
	}
}

// The failure path is BOUNDED. A remote that is broken rather than briefly unreachable would
// otherwise grow the directory without limit, and telemetry is not worth unbounded disk.
func TestStagedEventsAreBounded(t *testing.T) {
	store, _ := newTestS3Store(t)
	store.cacheBase = t.TempDir()

	for i := 0; i < maxStagedEvents+40; i++ {
		store.stageEvent(fmt.Sprintf("events/p/ast/%04d.json", i), []byte("{}"))
	}

	entries, err := os.ReadDir(filepath.Join(store.cacheBase, eventsStagingSubdir))
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) > maxStagedEvents {
		t.Errorf("staging holds %d events, want at most %d", len(entries), maxStagedEvents)
	}
}

// With no bucket there is no destination, so the event is dropped rather than queued: a queue with
// no consumer is a disk leak, not durability.
func TestWriteEventFileWithNoBucketDropsRatherThanQueues(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	store.cacheBase = t.TempDir()

	store.WriteEventFile(EventKey("", "", "global.setup", time.Now().UTC(), "01XYZ"), []byte("{}"))
	WaitForPendingEvents()
	store.SyncEvents(context.Background())

	if entries, err := os.ReadDir(filepath.Join(store.cacheBase, eventsStagingSubdir)); err == nil && len(entries) != 0 {
		t.Errorf("local-only mode queued %d events; nothing will ever drain them", len(entries))
	}
}

// The timestamp leads the object name so a listing is chronological without parsing bodies.
func TestEventKeyIsChronologicalAndScoped(t *testing.T) {
	early := EventKey("payments", "ast", "artifact.install", time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC), "A")
	late := EventKey("payments", "ast", "artifact.install", time.Date(2026, 8, 21, 3, 4, 5, 0, time.UTC), "B")
	if early >= late {
		t.Fatalf("keys do not sort chronologically: %q then %q", early, late)
	}
	if !strings.HasPrefix(early, "payments/ast/") {
		t.Fatalf("key is not scoped by project and type: %q", early)
	}

	fallback := EventKey("", "", "global.setup", time.Now().UTC(), "C")
	if !strings.HasPrefix(fallback, "_default/_none/") {
		t.Fatalf("unscoped event should fall back to _default/_none: %q", fallback)
	}
}

func TestRuleRoundTrip(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := context.Background()

	if err := store.WriteRule(ctx, "ast.md", []byte("# AST rule")); err != nil {
		t.Fatalf("WriteRule: %v", err)
	}
	if err := store.WriteRule(ctx, "memory.md", []byte("# Memory rule")); err != nil {
		t.Fatalf("WriteRule: %v", err)
	}

	names, err := store.ListRules(ctx)
	if err != nil {
		t.Fatalf("ListRules: %v", err)
	}
	if strings.Join(names, ",") != "ast.md,memory.md" {
		t.Fatalf("ListRules = %v", names)
	}

	data, err := store.ReadRule(ctx, "ast.md")
	if err != nil || string(data) != "# AST rule" {
		t.Fatalf("ReadRule = %q, %v", data, err)
	}
}

// A document from a newer publisher must be refused, not parsed into a shape it may not have.
func TestReadJSONRefusesANewerManifestVersion(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := context.Background()

	if err := store.WriteFile(ctx, "projects/ab/abcdef/ast_9.json", []byte(`{"v":99,"entry":{}}`)); err != nil {
		t.Fatal(err)
	}

	var out entryFile
	err := ReadJSON(ctx, store, "projects/ab/abcdef/ast_9.json", &out)
	if err == nil {
		t.Fatal("ReadJSON accepted a manifest version this build does not know")
	}
	if !strings.Contains(err.Error(), "newer publisher") {
		t.Fatalf("error should say why it was refused: %v", err)
	}
}

func TestReadJSONReadsTheCurrentManifestVersion(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := context.Background()

	if err := store.WriteFile(ctx, "projects/ab/abcdef/skill_x_1.0.0.json",
		[]byte(`{"v":1,"entry":{"id":"x","name":"X","type":"skill","latest":"1.0.0"}}`)); err != nil {
		t.Fatal(err)
	}

	var out entryFile
	if err := ReadJSON(ctx, store, "projects/ab/abcdef/skill_x_1.0.0.json", &out); err != nil {
		t.Fatalf("ReadJSON: %v", err)
	}
	if out.Entry.ID != "x" || out.Entry.Type != TypeSkill {
		t.Fatalf("decoded entry = %+v", out.Entry)
	}
}

// Every remote operation in local-only mode must say so with the sentinel, so callers can
// tell "no Hub configured" from "the Hub is broken".
func TestRemoteOperationsInLocalOnlyModeReturnErrNotConfigured(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	store, err := NewS3Store(context.Background(), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()

	if _, err := store.ReadFile(ctx, "x"); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Errorf("ReadFile: %v", err)
	}
	if err := store.WriteFile(ctx, "x", nil); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Errorf("WriteFile: %v", err)
	}
	if _, err := store.ListDir(ctx, "x"); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Errorf("ListDir: %v", err)
	}
	if err := store.PublishArtifact(ctx, TypeSkill, "a", "1", "", t.TempDir()); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Errorf("PublishArtifact: %v", err)
	}
	if _, err := store.ListRules(ctx); !errors.Is(err, s3store.ErrNotConfigured) {
		t.Errorf("ListRules: %v", err)
	}
	if uri := store.ArtifactURI(TypeAST, "a", "1", ""); uri != "" {
		t.Errorf("ArtifactURI in local-only mode = %q, want empty", uri)
	}
}

func TestIsMountableCoversExactlyTheTwoStoreTypes(t *testing.T) {
	for _, typ := range ValidTypes {
		want := typ == TypeAST || typ == TypeKnowledge
		if got := IsMountable(typ); got != want {
			t.Errorf("IsMountable(%s) = %v, want %v", typ, got, want)
		}
	}
}

func TestSyncRegistryOnAnEmptyBucketSucceeds(t *testing.T) {
	dir := t.TempDir()
	store, _ := newFakeBackedStore(t, dir)

	if err := store.SyncRegistry(context.Background()); err != nil {
		t.Fatalf("SyncRegistry on an empty bucket: %v", err)
	}

	info, err := os.Stat(store.RegistryMirrorDir())
	if err != nil || !info.IsDir() {
		t.Fatalf("expected an empty registry mirror to exist: %v", err)
	}
	entries, err := os.ReadDir(store.RegistryMirrorDir())
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Errorf("the mirror of an empty registry holds %d entries, want 0", len(entries))
	}

	if err := store.SyncRegistry(context.Background()); err != nil {
		t.Fatalf("second SyncRegistry: %v", err)
	}
}
