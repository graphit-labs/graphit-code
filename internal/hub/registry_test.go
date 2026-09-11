package hub

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

const (
	testProjectOne = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	testProjectTwo = "01BX5ZZKBKACTAV9WEVGEMMVRZ"
)

func trustedHubContext(t *testing.T, teams ...string) context.Context {
	t.Helper()
	ctx, err := hubaccess.WithTrustedSubject(context.Background(), hubaccess.Subject{UserID: "alice", TeamIDs: teams})
	if err != nil {
		t.Fatal(err)
	}
	return ctx
}

func allowProjects(t *testing.T, ctx context.Context, store *S3Store, selectors ...hubaccess.Selector) {
	t.Helper()
	data, err := json.Marshal(hubaccess.GrantDocument{Version: 1, Projects: selectors})
	if err != nil {
		t.Fatal(err)
	}
	if err := store.WriteFile(ctx, hubaccess.GlobalProjectsKey(), data); err != nil {
		t.Fatal(err)
	}
}

func registryForStore(ctx context.Context, store *S3Store) *RegistryManager {
	return &RegistryManager{store: store, baseCtx: ctx, entries: make(map[ArtifactType]map[string]*Entry), projects: make(map[string]*Project)}
}

func TestUpsertProjectReservesGlobalNameAndRenamesWithoutMovingData(t *testing.T) {
	store, fake := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	registry := registryForStore(ctx, store)

	project, err := registry.UpsertProject(ctx, testProjectOne, "payments-api", "Payments")
	if err != nil {
		t.Fatal(err)
	}
	if project.ID != testProjectOne || project.Revision != 1 || project.Status != "active" {
		t.Fatalf("created project = %#v", project)
	}
	payloadKey := hubaccess.ProjectArtifactPrefix(testProjectOne, "skills", "lint", "1.0.0") + "/SKILL.md"
	if err := store.WriteFile(ctx, payloadKey, []byte("payload")); err != nil {
		t.Fatal(err)
	}

	renamed, err := registry.UpsertProject(ctx, testProjectOne, "billing-api", "Billing")
	if err != nil {
		t.Fatal(err)
	}
	if renamed.Revision != 2 || renamed.Name != "billing-api" {
		t.Fatalf("renamed project = %#v", renamed)
	}
	if _, err := store.ReadFile(ctx, payloadKey); err != nil {
		t.Fatalf("project payload moved during rename: %v; keys=%v", err, fake.Keys())
	}
	if _, err := store.ReadFile(ctx, hubaccess.NameRecordKey("payments-api")); !errors.Is(err, s3store.ErrNotFound) {
		t.Fatalf("old name still resolves: %v", err)
	}
	resolved, err := registry.readNameProject(ctx, hubaccess.NameRecordKey("billing-api"))
	if err != nil || resolved.ID != testProjectOne {
		t.Fatalf("new name resolution = %#v, %v", resolved, err)
	}
}

func TestUpsertProjectRejectsNameCollisionWithConditionalCreate(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	registry := registryForStore(ctx, store)
	if _, err := registry.UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	if _, err := registry.UpsertProject(ctx, testProjectTwo, "payments-api", ""); err == nil {
		t.Fatal("second project acquired an existing friendly name")
	}
}

func TestDiscoverProjectsUsesExactAndPrefixGrantsWithPagination(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	registry := registryForStore(ctx, store)
	if _, err := registry.UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	if _, err := registry.UpsertProject(ctx, testProjectTwo, "platform-api", ""); err != nil {
		t.Fatal(err)
	}
	allowProjects(t, ctx, store, hubaccess.Selector{ID: testProjectTwo}, hubaccess.Selector{NamePrefix: "payments-"})

	first, err := registry.DiscoverProjects(ctx, 1, "")
	if err != nil {
		t.Fatal(err)
	}
	if len(first.Projects) != 1 || first.Projects[0].ID != testProjectTwo || first.NextCursor == "" {
		t.Fatalf("first page = %#v", first)
	}
	second, err := registry.DiscoverProjects(ctx, 1, first.NextCursor)
	if err != nil {
		t.Fatal(err)
	}
	if len(second.Projects) != 1 || second.Projects[0].ID != testProjectOne {
		t.Fatalf("second page = %#v", second)
	}
}

func TestResolveProjectUsesGlobalNameDirectoryAndRevalidatesGrant(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	registry := registryForStore(ctx, store)
	if _, err := registry.UpsertProject(ctx, testProjectOne, "payments-api", ""); err != nil {
		t.Fatal(err)
	}
	allowProjects(t, ctx, store, hubaccess.Selector{NamePrefix: "payments-"})
	project, err := registry.ResolveProject(ctx, "payments-api")
	if err != nil {
		t.Fatal(err)
	}
	if project.ID != testProjectOne {
		t.Fatalf("resolved project = %#v", project)
	}
	allowProjects(t, ctx, store)
	if _, err := registry.ResolveProject(ctx, testProjectOne); !errors.Is(err, hubaccess.ErrDenied) {
		t.Fatalf("revoked exact lookup error = %v", err)
	}
}

func TestProjectScopedArtifactIDsDoNotCollide(t *testing.T) {
	registry := &RegistryManager{entries: map[ArtifactType]map[string]*Entry{TypeSkill: {}}}
	one := &Entry{ID: "lint", Type: TypeSkill, ProjectID: testProjectOne}
	two := &Entry{ID: "lint", Type: TypeSkill, ProjectID: testProjectTwo}
	registry.entries[TypeSkill][entryKey(one.ProjectID, one.ID)] = one
	registry.entries[TypeSkill][entryKey(two.ProjectID, two.ID)] = two
	if registry.GetEntry("lint", TypeSkill) != nil {
		t.Fatal("ambiguous bare artifact ID resolved across projects")
	}
	if got := registry.GetEntryInProject(testProjectTwo, "lint", TypeSkill); got != two {
		t.Fatalf("project-scoped lookup = %#v", got)
	}
}

func TestListEntriesPageBoundsArtifactDiscovery(t *testing.T) {
	store, _ := newTestS3Store(t)
	ctx := trustedHubContext(t)
	allowProjects(t, ctx, store, hubaccess.Selector{All: true})
	registry := registryForStore(ctx, store)
	for _, project := range []struct {
		id   string
		name string
	}{
		{id: testProjectOne, name: "payments-api"},
		{id: testProjectTwo, name: "platform-api"},
	} {
		if _, err := registry.UpsertProject(ctx, project.id, project.name, ""); err != nil {
			t.Fatal(err)
		}
		file := entryFile{Version: hubManifestVersion, Entry: Entry{ID: "lint", Name: "Lint", Type: TypeSkill, ProjectID: project.id}}
		data, err := json.Marshal(file)
		if err != nil {
			t.Fatal(err)
		}
		if err := store.WriteFile(ctx, hubaccess.ProjectRegistryKey(project.id, string(TypeSkill), "lint"), data); err != nil {
			t.Fatal(err)
		}
	}

	first, err := registry.ListEntriesPage(ctx, TypeSkill, 1, "")
	if err != nil {
		t.Fatal(err)
	}
	if len(first.Entries) != 1 || first.NextCursor == "" {
		t.Fatalf("first page = %#v", first)
	}
	second, err := registry.ListEntriesPage(ctx, TypeSkill, 1, first.NextCursor)
	if err != nil {
		t.Fatal(err)
	}
	if len(second.Entries) != 1 || second.Entries[0].ProjectID == first.Entries[0].ProjectID {
		t.Fatalf("second page = %#v", second)
	}
}
