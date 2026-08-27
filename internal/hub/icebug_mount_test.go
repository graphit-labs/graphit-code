package hub

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
)

// THE FULL CYCLE for an AST artifact: index, publish as icebug, mount the DDL, query it.
//
// Mounted against a LOCAL storage URI here, which is the same code path with a different scheme —
// the DDL, the catalog and the query are identical, and only the object transport differs. The
// network transport is covered where it belongs, in lancestore's on-the-fly test and in
// TestPublishedWikiIsReadDirectlyFromObjectStorage.
func TestIcebugArtifactMountsAndAnswers(t *testing.T) {
	ctx := context.Background()
	tmp := t.TempDir()

	// ---- a real graph, from real source ----
	work := filepath.Join(tmp, "src")
	if err := os.MkdirAll(work, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(work, "main.go"), []byte(`package main

// Run starts the process.
func Run() { helper() }

// helper does the work Run asks for.
func helper() {}
`), 0o644); err != nil {
		t.Fatal(err)
	}

	storeDir := filepath.Join(tmp, "store")
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	db := ast.NewLadybugDB(ast.LadybugConfig{DBPath: filepath.Join(storeDir, "ladybugdb")})
	// IndexSource is opt-in and OFF by default, so it has to be asked for here: the search index
	// is the only queryable copy of file text, and without it the assertion below would be testing
	// a store that was never told to keep any.
	pipelineResult, err := ast.RunPipeline(ctx, db, work,
		ast.PipelineOptions{CacheDir: storeDir, IndexSource: true})
	if err != nil {
		_ = db.Close()
		t.Fatalf("index: %v", err)
	}
	if pipelineResult.WriteErrorCount != 0 {
		_ = db.Close()
		t.Fatalf("index write failed for %d file(s): %v",
			pipelineResult.WriteErrorCount, pipelineResult.WriteErrorFiles)
	}
	if pipelineResult.TotalFiles != 1 || pipelineResult.ParsedFiles != 1 {
		_ = db.Close()
		t.Fatalf("index did not parse the fixture: %+v", pipelineResult)
	}
	_ = db.Close()

	// ---- publish it, with storage pointing where the consumer will read ----
	published := filepath.Join(tmp, "published")
	if err := os.MkdirAll(published, 0o755); err != nil {
		t.Fatal(err)
	}
	staged, err := prepareASTPublish(storeDir, ast.IcebugBundlePath(published), nil, nil)
	if err != nil {
		t.Fatalf("publish: %v", err)
	}
	defer func() { _ = os.RemoveAll(staged) }()

	// The upload, as the Hub would do it: the bundle lands at the location its own DDL names.
	if err := os.RemoveAll(published); err != nil {
		t.Fatal(err)
	}
	if err := copyTree(ast.IcebugBundlePath(staged), ast.IcebugBundlePath(published)); err != nil {
		t.Fatalf("staging the published bundle: %v", err)
	}

	schema, err := os.ReadFile(filepath.Join(ast.IcebugBundlePath(published), ast.IcebugSchemaFile))
	if err != nil {
		t.Fatalf("published schema: %v", err)
	}

	// ---- mount: a FRESH catalog, and not one byte of graph copied ----
	mountDir := filepath.Join(tmp, "mounted")
	mountDB := filepath.Join(mountDir, "ladybugdb")
	if err := ast.MountIcebugGraph(ctx, mountDB, string(schema), nil); err != nil {
		t.Fatalf("mount: %v", err)
	}

	// THE DATA DID NOT COME DOWN, asserted by what is absent rather than by size.
	//
	// Size was the first thing tried and it proves nothing: a catalog has a floor — one database
	// page, 16 KiB here — so against a two-function graph the catalog is legitimately the larger
	// of the two. On a real graph the ratio is the other way round by orders of magnitude, but a
	// test that only holds for large inputs is a test that will be wrong on someone's fixture.
	//
	// What is checkable at any size: the CSR files exist in the published bundle and nowhere under
	// the mount. Those are the graph.
	published_files := dataFiles(t, ast.IcebugBundlePath(published))
	if len(published_files) == 0 {
		t.Fatal("the published bundle holds no data files; the export produced nothing to mount")
	}
	mounted_files := dataFiles(t, mountDir)
	if len(mounted_files) > 0 {
		t.Errorf("mounting copied %d data file(s) into the local catalog — the graph came down, "+
			"which is what mounting exists to avoid: %v", len(mounted_files), mounted_files)
	}
	t.Logf("published bundle holds %d data files; the local catalog holds %d",
		len(published_files), len(mounted_files))
	t.Logf("published graph: %d bytes; local catalog after mounting: %d bytes",
		treeSize(t, ast.IcebugBundlePath(published)), treeSize(t, mountDir))

	// ---- query the mounted graph ----
	mounted := ast.NewLadybugDBReadOnly(ast.LadybugConfig{DBPath: mountDB})
	defer func() { _ = mounted.Close() }()

	res, err := mounted.Query(ctx, "MATCH (n) RETURN count(n) AS n", nil)
	if err != nil {
		t.Fatalf("counting nodes on the mounted graph: %v", err)
	}
	if len(res.Records) == 0 {
		t.Fatal("counting nodes on the mounted graph returned no rows")
	}
	nodes := asInt(res.Records[0]["n"])
	if nodes == 0 {
		t.Fatal("the mounted graph has no nodes — the storage clause is not resolving")
	}
	t.Logf("mounted graph answers: %d nodes", nodes)

	// A single-hop traversal, which is what a mounted graph serves. The label is a COLUMN here,
	// not a table — see internal/ast/icebug_transfer.go for why the fold is forced.
	// ---- THE SEARCH HALF, which works without being mounted and then answers nothing ----
	//
	// A context with a mounted graph and a LOCAL search path traverses perfectly and returns no
	// search results — indistinguishable from a corpus with no match. So the mount records where
	// the index is, and this asserts that a search actually reaches it.
	searchDir := filepath.Join(published, ast.SearchBundleDir)
	if err := copyTree(filepath.Join(staged, ast.SearchBundleDir), searchDir); err != nil {
		t.Fatalf("staging the published search index: %v", err)
	}
	if err := ast.WriteSearchMount(mountDB, searchDir); err != nil {
		t.Fatalf("recording the search mount: %v", err)
	}
	// Nothing of the index is beside the catalog — the local directory the unmounted path would
	// have used must not exist, or this test could pass on a local copy.
	if _, err := os.Stat(ast.LanceIndexPath(mountDB)); err == nil {
		t.Fatal("a local search index exists beside the mounted catalog; this test would prove nothing")
	}

	idx, err := ast.OpenSearchIndex(ctx, mountDB)
	if err != nil {
		t.Fatalf("opening the mounted search index: %v", err)
	}
	defer func() { _ = idx.Close() }()

	hits, err := idx.Search(ctx, "helper", 5)
	if err != nil {
		t.Fatalf("searching the mounted index: %v", err)
	}
	var foundHelper bool
	for _, h := range hits {
		if h.Name == "helper" {
			foundHelper = true
		}
	}
	if !foundHelper {
		t.Errorf("searching the mounted index did not find the entity it was published with: %+v", hits)
	} else {
		t.Logf("mounted search answers: %d hits for \"helper\"", len(hits))
	}

	// And the file text, which is the only queryable copy there is.
	if src, ok := idx.FileSource(ctx, "main.go"); !ok || !strings.Contains(src, "func helper") {
		t.Errorf("the mounted index does not serve file text for main.go (ok=%v, len=%d)", ok, len(src))
	} else {
		t.Log("mounted index serves file text")
	}

	res, err = mounted.Query(ctx,
		"MATCH ()-[r:calls__function_function]->() RETURN count(r) AS n", nil)
	if err != nil {
		t.Fatalf("traversing the mounted graph: %v", err)
	}
	if len(res.Records) == 0 || asInt(res.Records[0]["n"]) == 0 {
		t.Error("a single-hop traversal over the mounted graph found nothing; the CSR is not being read")
	} else {
		t.Logf("mounted graph answers: %d CALLS edges", asInt(res.Records[0]["n"]))
	}
}

// Mounting a schema that names nothing has to fail, rather than leaving an empty catalog that
// answers every query with zero rows.
func TestMountRefusesAnEmptySchema(t *testing.T) {
	err := ast.MountIcebugGraph(context.Background(),
		filepath.Join(t.TempDir(), "ladybugdb"), "\n// only a comment\n", nil)
	if err == nil {
		t.Fatal("mounting an empty schema succeeded")
	}
	if !strings.Contains(err.Error(), "no statements") {
		t.Errorf("the refusal does not say what was wrong: %v", err)
	}
}

func copyTree(src, dst string) error {
	return filepath.Walk(src, func(path string, fi os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, rErr := filepath.Rel(src, path)
		if rErr != nil {
			return rErr
		}
		target := filepath.Join(dst, rel)
		if fi.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, rErr := os.ReadFile(path)
		if rErr != nil {
			return rErr
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
}

// dataFiles lists the Parquet files under dir — the CSRs and the node table, which is what the
// graph actually IS. Anything else in an icebug directory is metadata.
func dataFiles(t *testing.T, dir string) []string {
	t.Helper()
	var out []string
	_ = filepath.Walk(dir, func(path string, fi os.FileInfo, err error) error {
		if err != nil || fi.IsDir() {
			return nil
		}
		if strings.EqualFold(filepath.Ext(path), ".parquet") {
			rel, _ := filepath.Rel(dir, path)
			out = append(out, rel)
		}
		return nil
	})
	return out
}

func treeSize(t *testing.T, dir string) int64 {
	t.Helper()
	var total int64
	_ = filepath.Walk(dir, func(_ string, fi os.FileInfo, err error) error {
		if err == nil && !fi.IsDir() {
			total += fi.Size()
		}
		return nil
	})
	return total
}

func asInt(v any) int64 {
	switch n := v.(type) {
	case int64:
		return n
	case int:
		return int64(n)
	case float64:
		return int64(n)
	}
	return 0
}
