package hub

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/version"
)

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
	t.Setenv("GRAPHIT_AI_EMBEDDING_PROVIDER", "openai")
	t.Setenv("GRAPHIT_AI_EMBEDDING_MODEL", "text-embedding-3-small")
	original := version.Version
	t.Cleanup(func() { version.Version = original })

	version.Version = "1.0.0"
	first := lanceFingerprint(TypeAST)
	version.Version = "latest"
	if second := lanceFingerprint(TypeAST); second != first {
		t.Fatalf("producer version changed semantic fingerprint: %s != %s", first, second)
	}
	t.Setenv("GRAPHIT_AI_EMBEDDING_MODEL", "text-embedding-3-large")
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
