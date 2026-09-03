package hub

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/config"
)

// ICEBUG IS THE ONLY SHAPE an AST artifact takes, and this is what pins that.
//
// Two fallbacks were removed to get here: the Parquet bundle, which made the consumer LOAD the
// graph, and the parse shards, which made it REBUILD one. Both meant an artifact's behaviour
// depended on which path it happened to take, so a consumer could not know whether its context
// was mounted or copied. What replaced them is a graph that stays where it was published.
//
// So a store that cannot be exported now FAILS to publish. That is the deliberate part: an
// artifact nobody can mount is one nobody can install the intended way, and publish time is the
// cheap place to find out.
func TestPrepareASTPublishProducesOnlyIcebug(t *testing.T) {
	tmp := t.TempDir()
	work := filepath.Join(tmp, "src")
	if err := os.MkdirAll(work, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(work, "main.go"),
		[]byte("package main\n\n// Run does the thing.\nfunc Run() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	storeDir := filepath.Join(tmp, "store")
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	icebugDir := filepath.Join(storeDir, "graph.icebug")
	db := ast.NewLadybugDB(ast.LadybugConfig{StoreDir: storeDir, IcebugDir: icebugDir})
	if _, err := ast.RunPipeline(context.Background(), db, work,
		ast.PipelineOptions{CacheDir: storeDir}); err != nil {
		_ = db.Close()
		t.Fatalf("index: %v", err)
	}
	_ = db.Close()

	const storageURI = "s3://example-bucket/artifacts/ast/proj/1.0.0/" + ast.IcebugBundleDir
	if raw, rerr := os.ReadFile(filepath.Join(storeDir, "graph.icebug", "schema.cypher")); rerr == nil {
		t.Logf("local schema has reverse: %v", strings.Contains(string(raw), "reverse"))
	}
	staged, err := prepareASTPublish(storeDir, storageURI, nil, nil)
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	defer func() { _ = os.RemoveAll(staged) }()

	if !ast.HasIcebugBundle(staged) {
		t.Fatalf("a built store published no icebug bundle; staged contents: %v",
			dirNames(t, staged))
	}
	if _, err := os.Stat(filepath.Join(staged, "shards")); err == nil {
		t.Error("shards were staged alongside the graph, doubling the artifact")
	}

	schema, err := os.ReadFile(filepath.Join(ast.IcebugBundlePath(staged), ast.IcebugSchemaFile))
	if err != nil {
		t.Fatalf("the published bundle has no %s: %v", ast.IcebugSchemaFile, err)
	}
	ddl := string(schema)
	if !strings.Contains(ddl, storageURI) {
		t.Errorf("the published schema does not point at the consumer's location %q:\n%s",
			storageURI, ddl)
	}
	if !strings.Contains(ddl, "format = 'icebug-disk'") {
		t.Errorf("the published schema does not declare the icebug format:\n%s", ddl)
	}
	if strings.Contains(ddl, staged) || strings.Contains(ddl, storeDir) {
		t.Errorf("the published schema leaks a local path, so it would only mount here:\n%s", ddl)
	}
	if n := strings.Count(ddl, "CREATE "); n < 2 {
		t.Errorf("the published schema has %d CREATE statements, want the node table and at "+
			"least one relationship table:\n%s", n, ddl)
	}
	if !strings.Contains(strings.ToLower(ddl), "_reverse") {
		t.Errorf("the default publication omitted reverse relationship tables:\n%s", ddl)
	}

	storeDir2 := filepath.Join(tmp, "store2")
	icebugDir2 := filepath.Join(storeDir2, "graph.icebug")
	off := false
	db2 := ast.NewLadybugDB(ast.LadybugConfig{StoreDir: storeDir2, IcebugDir: icebugDir2})
	if _, err := ast.RunPipeline(context.Background(), db2, work,
		ast.PipelineOptions{CacheDir: storeDir2, ReverseEdges: &off}); err != nil {
		_ = db2.Close()
		t.Fatalf("index without reverse: %v", err)
	}
	_ = db2.Close()

	withoutReverse, err := prepareASTPublish(storeDir2, storageURI, config.ConfigMap{
		"hub": map[string]any{"icebug.reverse_edges": "false"},
	}, nil)
	if err != nil {
		t.Fatalf("prepare with reverse edges disabled: %v", err)
	}
	defer func() { _ = os.RemoveAll(withoutReverse) }()
	withoutReverseSchema, err := os.ReadFile(filepath.Join(
		ast.IcebugBundlePath(withoutReverse), ast.IcebugSchemaFile))
	if err != nil {
		t.Fatalf("schema with reverse edges disabled: %v", err)
	}
	if strings.Contains(string(withoutReverseSchema), "_REVERSE") {
		t.Errorf("hub.icebug.reverse_edges=false still published reverse tables:\n%s",
			withoutReverseSchema)
	}
}

// A store with no graph cannot be published, and the refusal has to name why.
func TestPrepareASTPublishRefusesAStoreWithNoGraph(t *testing.T) {
	bare := t.TempDir()
	if err := os.MkdirAll(filepath.Join(bare, "shards"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(bare, "shards", "a.go.nodes.json"),
		[]byte(`{"v":7}`), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := prepareASTPublish(bare, "s3://b/p", nil, nil)
	if err == nil {
		t.Fatal("a store with no graph was published; the shard fallback is supposed to be gone")
	}
	if !strings.Contains(err.Error(), "no graph") {
		t.Errorf("the refusal does not say what is missing: %v", err)
	}
}

// The storage URI is not optional, and an empty one has to be refused rather than written as an
// empty clause — a table with `storage = ”` mounts against the process's working directory.
func TestPrepareASTPublishRefusesAnEmptyStorageURI(t *testing.T) {
	tmp := t.TempDir()
	storeDir := filepath.Join(tmp, "store")
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(storeDir, "ladybugdb"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := prepareASTPublish(storeDir, "", nil, nil); err == nil {
		t.Error("an artifact was published with no storage location")
	}
}

func dirNames(t *testing.T, dir string) []string {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []string
	for _, e := range entries {
		out = append(out, e.Name())
	}
	return out
}
