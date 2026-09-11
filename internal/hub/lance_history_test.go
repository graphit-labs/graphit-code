package hub

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/version"
)

func TestHydrateProjectLanceSkipsNonGitProject(t *testing.T) {
	if err := HydrateProjectLance(context.Background(), t.TempDir(), nil); err != nil {
		t.Fatalf("hydrate non-Git project: %v", err)
	}
}

func TestHydrationTreatsMissingProjectMetadataAsNoPublishedBase(t *testing.T) {
	store, _ := newTestS3Store(t)
	published, err := authorizeHydrationProject(context.Background(), store, testProjectOne)
	if err != nil {
		t.Fatal(err)
	}
	if published {
		t.Fatal("missing project metadata was treated as a published project")
	}

	if err := store.WriteFile(context.Background(), hubaccess.ProjectMetadataKey(testProjectOne), []byte(`{"v":2}`)); err != nil {
		t.Fatal(err)
	}
	if _, err := authorizeHydrationProject(context.Background(), store, testProjectOne); err == nil {
		t.Fatal("malformed project metadata did not fail closed")
	}
}

func TestInitializedLanceStoreProtectsExistingLocalTables(t *testing.T) {
	storePath := filepath.Join(t.TempDir(), "search.lance")
	if err := os.MkdirAll(filepath.Join(storePath, "entities.lance"), 0o755); err != nil {
		t.Fatal(err)
	}
	if !initializedLanceStore(storePath) {
		t.Fatal("existing local Lance table was not recognized")
	}
}

func TestSelectLanceBaseUsesNearestCompatibleAncestor(t *testing.T) {
	history := lanceBranchHistory{Commits: []lanceCommit{
		{Commit: "unrelated", Fingerprint: "compatible"},
		{Commit: "parent", Fingerprint: "compatible"},
		{Commit: "head", Fingerprint: "old-format"},
	}}
	got, ok := selectLanceBase(history, []string{"head", "parent", "root"}, "compatible")
	if !ok || got.Commit != "parent" {
		t.Fatalf("base = %#v, %v", got, ok)
	}
}

func TestLanceFingerprintIgnoresProducerVersion(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, AI: auth.AIConfig{
		Embedding: auth.AIServiceConfig{Mode: auth.ServiceDirect, Protocol: "openai", Endpoint: "https://api.example.test/v1", Model: "text-embedding-3-small", Dimensions: 1536},
		Rerank:    auth.AIServiceConfig{Mode: auth.ServiceDisabled},
	}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "profile", Provider: "test", Username: "test", EmbeddingAPIKey: "synthetic"}); err != nil {
		t.Fatal(err)
	}
	original := version.Version
	t.Cleanup(func() { version.Version = original })

	version.Version = "1.0.0"
	first := lanceFingerprint(TypeAST)
	version.Version = "latest"
	if second := lanceFingerprint(TypeAST); second != first {
		t.Fatalf("producer version changed semantic fingerprint: %s != %s", first, second)
	}
	provider.AI.Embedding.Model = "text-embedding-3-large"
	provider.AI.Embedding.Dimensions = 3072
	if err := store.UpdateProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "profile", Provider: "test", Username: "test", EmbeddingAPIKey: "synthetic"}); err != nil {
		t.Fatal(err)
	}
	if changed := lanceFingerprint(TypeAST); changed == first {
		t.Fatal("embedding model did not change semantic fingerprint")
	}
}

func TestArtifactPathIsLancePreservesNestedDatasetsOnly(t *testing.T) {
	for _, path := range []string{"search.lance/_versions/1.manifest", "wiki/index.lance/data/one.lance"} {
		if !artifactPathIsLance(path) {
			t.Fatalf("%q was not recognized as Lance data", path)
		}
	}
	if artifactPathIsLance("graph.icebug/schema.cypher") {
		t.Fatal("non-Lance artifact file was preserved")
	}
}

func TestValidateLanceHistoryRejectsAnotherBranch(t *testing.T) {
	history := lanceBranchHistory{Version: 1, ProjectID: "project", ArtifactType: TypeAST, Branch: "feature/one"}
	if err := validateLanceHistory(history, TypeAST, "project", "feature/two"); err == nil {
		t.Fatal("expected branch ownership mismatch")
	}
	if err := validateLanceHistory(history, TypeAST, "project", "feature/one"); err != nil {
		t.Fatal(err)
	}
}

func TestLanceTableIndexesMatchPublishedSchemas(t *testing.T) {
	files := lanceTableIndexes("files", nil)
	if len(files) != 2 || files[0].Column != "source" {
		t.Fatalf("file indexes = %#v", files)
	}
	rows := make([]lancestore.Row, 256)
	for i := range rows {
		rows[i] = lancestore.Row{"embedding": []float32{1}}
	}
	entities := lanceTableIndexes("entities", rows)
	if got := entities[len(entities)-1]; got.Column != "embedding" || got.Kind != lancestore.IndexVectorIVFPQ {
		t.Fatalf("last entity index = %#v", got)
	}
}
