package ast

import (
	"context"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

func TestMountedIcebugPlansBoundedInboundTraversalAsOneHopFrontiers(t *testing.T) {
	db := mountedIcebugTraversalFixture(t)
	ctx := context.Background()

	result, err := db.Query(ctx,
		"MATCH (caller)-[:CALLS*1..3]->(t) WHERE t.name = 'target' "+
			"RETURN DISTINCT caller.name, caller.path", nil)
	if err != nil {
		t.Fatalf("bounded inbound traversal: %v", err)
	}
	if got, want := recordStrings(result, "caller.name"), []string{"caller1", "caller2", "caller3"}; !sameStrings(got, want) {
		t.Fatalf("callers = %v, want %v", got, want)
	}
	pathSet := map[string]bool{}
	for _, pth := range recordStrings(result, "caller.path") {
		pathSet[pth] = true
	}
	if len(pathSet) != 2 || !pathSet["caller1.go"] || !pathSet["shared.go"] {
		t.Fatalf("path set = %v, want {caller1.go shared.go}", pathSet)
	}
}

func TestMountedIcebugBoundedTraversalKeepsEndpointFiltersAndParameters(t *testing.T) {
	db := mountedIcebugTraversalFixture(t)
	ctx := context.Background()

	result, err := db.Query(ctx,
		"MATCH (caller)-[:CALLS*1..3]->(t) "+
			"WHERE (t.name = $target) AND caller.name CONTAINS '2' "+
			"RETURN DISTINCT caller.name", map[string]any{"target": "target"})
	if err != nil {
		t.Fatalf("bounded traversal with endpoint filters: %v", err)
	}
	if got, want := recordStrings(result, "caller.name"), []string{"caller2"}; !sameStrings(got, want) {
		t.Fatalf("filtered callers = %v, want %v", got, want)
	}
}

func TestMountedIcebugPlansBoundedOutboundTraversal(t *testing.T) {
	db := mountedIcebugTraversalFixture(t)

	result, err := db.Query(context.Background(),
		"MATCH (root:Function {name: 'caller3'})-[:CALLS*1..3]->(callee) "+
			"RETURN DISTINCT callee.name", nil)
	if err != nil {
		t.Fatalf("bounded outbound traversal: %v", err)
	}
	if got, want := recordStrings(result, "callee.name"), []string{"caller1", "caller2", "target"}; !sameStrings(got, want) {
		t.Fatalf("callees = %v, want %v", got, want)
	}
}

func TestMountedIcebugDirectionlessTraversalUsesBothCSRs(t *testing.T) {
	db := mountedIcebugTraversalFixture(t)

	result, err := db.Query(context.Background(),
		"MATCH (peer)-[:CALLS]-(center:Function {name: 'caller2'}) "+
			"RETURN DISTINCT peer.name", nil)
	if err != nil {
		t.Fatalf("directionless traversal: %v", err)
	}
	if got, want := recordStrings(result, "peer.name"), []string{"caller1", "caller3"}; !sameStrings(got, want) {
		t.Fatalf("directionless peers = %v, want %v", got, want)
	}
}

func TestMountedIcebugBoundedTraversalPreservesGlobalDistinct(t *testing.T) {
	db := mountedIcebugTraversalFixture(t)

	result, err := db.Query(context.Background(),
		"MATCH (caller)-[:CALLS*1..3]->(t {name: 'target'}) "+
			"RETURN DISTINCT caller.path", nil)
	if err != nil {
		t.Fatalf("bounded traversal DISTINCT: %v", err)
	}
	if got, want := recordStrings(result, "caller.path"), []string{"caller1.go", "shared.go"}; !sameStrings(got, want) {
		t.Fatalf("distinct paths = %v, want %v", got, want)
	}
}



func TestMountedIcebugRealGraphBoundedTraversalCost(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}
	query := "MATCH (caller)-[:CALLS*1..3]->(t) WHERE " +
		"(label(t) = 'Function' OR label(t) = 'Method') AND " +
		"t.uid IN ['internal/ast/ladybug.go::runQuery'] RETURN DISTINCT caller.uid"

	native := NewLadybugDBReadOnly(LadybugConfig{DBPath: storePath})
	if err := native.connect(); err != nil {
		t.Fatalf("open native graph: %v", err)
	}
	want, err := native.Query(context.Background(), query, nil)
	if err != nil {
		t.Fatalf("native query: %v", err)
	}
	if err := native.Close(); err != nil {
		t.Fatalf("close native graph: %v", err)
	}

	bundle := filepath.Join(t.TempDir(), "graph.icebug")
	if _, err := ExportGraphToIcebug(storePath, bundle, "", bundle, true, nil); err != nil {
		t.Fatalf("export Icebug: %v", err)
	}
	schema, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("read Icebug schema: %v", err)
	}
	mountPath := filepath.Join(t.TempDir(), "mounted.lbug")
	if err := MountIcebugGraph(context.Background(), mountPath, string(schema), nil); err != nil {
		t.Fatalf("mount Icebug: %v", err)
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{DBPath: mountPath})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted graph: %v", err)
	}
	defer func() { _ = mounted.Close() }()
	start := time.Now()
	got, err := mounted.Query(context.Background(), query, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("mounted query: %v", err)
	}
	t.Logf("bounded 3-hop traversal over Icebug: rows=%d took=%s", len(got.Records), took)
	if gotUIDs, wantUIDs := recordStrings(got, "caller.uid"), recordStrings(want, "caller.uid"); !sameStrings(gotUIDs, wantUIDs) {
		t.Fatalf("mounted callers = %v, native callers = %v", gotUIDs, wantUIDs)
	}
	if len(got.Records) == 0 {
		t.Fatal("bounded traversal returned no callers")
	}
	if took > 5*time.Second {
		t.Fatalf("bounded 3-hop traversal took %s, want <= 5s", took)
	}
}

func TestMountedIcebugRemoteRealGraphBoundedTraversalCost(t *testing.T) {
	if os.Getenv("GRAPHIT_REMOTE_ICEBUG_TEST") == "" {
		t.Skip("set GRAPHIT_REMOTE_ICEBUG_TEST=1 with a configured Hub bucket")
	}
	globalDir := os.Getenv("GRAPHIT_REMOTE_GLOBAL_DIR")
	if globalDir == "" {
		t.Skip("set GRAPHIT_REMOTE_GLOBAL_DIR to the installed Graphit global directory")
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}
	ctx := context.Background()
	objectStore, err := s3store.New(ctx, config.HubS3Config())
	if err != nil {
		t.Fatalf("open Hub object store: %v", err)
	}
	if err := objectStore.EnsureBucket(ctx); err != nil {
		t.Fatalf("reach Hub bucket: %v", err)
	}
	prefix := s3store.JoinKey("diagnostics", "icebug-three-hop-"+strconv.FormatInt(time.Now().UnixNano(), 10))
	t.Cleanup(func() {
		if err := objectStore.DeletePrefix(context.Background(), prefix); err != nil {
			t.Errorf("delete remote diagnostic prefix: %v", err)
		}
	})

	query := "MATCH (caller)-[:CALLS*1..3]->(t) WHERE " +
		"(label(t) = 'Function' OR label(t) = 'Method') AND " +
		"t.uid IN ['internal/ast/ladybug.go::runQuery'] RETURN DISTINCT caller.uid"
	native := NewLadybugDBReadOnly(LadybugConfig{DBPath: storePath})
	if err := native.connect(); err != nil {
		t.Fatalf("open native graph: %v", err)
	}
	want, err := native.Query(ctx, query, nil)
	if err != nil {
		t.Fatalf("native query: %v", err)
	}
	if err := native.Close(); err != nil {
		t.Fatalf("close native graph: %v", err)
	}

	bundle := filepath.Join(t.TempDir(), "graph.icebug")
	if _, err := ExportGraphToIcebug(storePath, bundle, "", objectStore.URI(prefix), true, nil); err != nil {
		t.Fatalf("export remote Icebug schema: %v", err)
	}
	if err := objectStore.UploadDir(ctx, bundle, prefix); err != nil {
		t.Fatalf("upload Icebug bundle: %v", err)
	}
	schema, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("read remote Icebug schema: %v", err)
	}
	mountPath := filepath.Join(t.TempDir(), "mounted.lbug")
	if err := MountIcebugGraph(ctx, mountPath, string(schema), nil); err != nil {
		t.Fatalf("mount remote Icebug: %v", err)
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{DBPath: mountPath})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open remote mounted graph: %v", err)
	}
	defer func() { _ = mounted.Close() }()

	start := time.Now()
	got, err := mounted.Query(ctx, query, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("remote mounted query: %v", err)
	}
	t.Logf("bounded 3-hop traversal over S3 Icebug: rows=%d took=%s", len(got.Records), took)
	if gotUIDs, wantUIDs := recordStrings(got, "caller.uid"), recordStrings(want, "caller.uid"); !sameStrings(gotUIDs, wantUIDs) {
		t.Fatalf("remote mounted callers = %v, native callers = %v", gotUIDs, wantUIDs)
	}
	if len(got.Records) == 0 {
		t.Fatal("remote bounded traversal returned no callers")
	}
	if took > 10*time.Second {
		t.Fatalf("remote bounded 3-hop traversal took %s, want <= 10s", took)
	}
}

func mountedIcebugTraversalFixture(t *testing.T) *LadybugBackend {
	t.Helper()
	srcPath := filepath.Join(t.TempDir(), "source.lbug")
	src := NewLadybugDB(LadybugConfig{DBPath: srcPath})
	if err := src.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	if err := src.initSchemaForLabels(SchemaInfo{
		Labels:       []string{"Function"},
		CallerLabels: []string{"Function"},
		CalleeLabels: []string{"Function"},
	}); err != nil {
		t.Fatalf("schema: %v", err)
	}
	ctx := context.Background()
	for _, node := range []struct{ uid, name, path string }{
		{"target", "target", "target.go"},
		{"caller1", "caller1", "caller1.go"},
		{"caller2", "caller2", "shared.go"},
		{"caller3", "caller3", "shared.go"},
	} {
		if _, err := src.Query(ctx,
			"CREATE (n:Function {uid: $uid, name: $name, path: $path}) RETURN n.uid",
			map[string]any{"uid": node.uid, "name": node.name, "path": node.path}); err != nil {
			t.Fatalf("insert %s: %v", node.uid, err)
		}
	}
	for _, edge := range [][2]string{{"caller1", "target"}, {"caller2", "caller1"}, {"caller3", "caller2"}} {
		if _, err := src.Query(ctx,
			"MATCH (a:Function {uid: $from}), (b:Function {uid: $to}) CREATE (a)-[:CALLS]->(b)",
			map[string]any{"from": edge[0], "to": edge[1]}); err != nil {
			t.Fatalf("insert edge %s -> %s: %v", edge[0], edge[1], err)
		}
	}
	if err := src.Close(); err != nil {
		t.Fatalf("close source: %v", err)
	}

	bundle := filepath.Join(t.TempDir(), "graph.icebug")
	if _, err := ExportGraphToIcebug(srcPath, bundle, "", bundle, true, nil); err != nil {
		t.Fatalf("export Icebug: %v", err)
	}
	schema, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("read Icebug schema: %v", err)
	}
	mountPath := filepath.Join(t.TempDir(), "mounted.lbug")
	if err := MountIcebugGraph(ctx, mountPath, string(schema), nil); err != nil {
		t.Fatalf("mount Icebug: %v", err)
	}
	if manifestRaw, mErr := os.ReadFile(filepath.Join(bundle, ladybug.IcebugManifestFile)); mErr == nil {
		if wErr := os.WriteFile(filepath.Join(filepath.Dir(mountPath), ladybug.IcebugManifestFile), manifestRaw, 0o644); wErr != nil {
			t.Fatalf("stage manifest: %v", wErr)
		}
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{DBPath: mountPath})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted Icebug: %v", err)
	}
	t.Cleanup(func() { _ = mounted.Close() })
	return mounted
}

func recordStrings(result *QueryResult, column string) []string {
	values := make([]string, 0, len(result.Records))
	for _, record := range result.Records {
		if value, ok := record[column].(string); ok {
			values = append(values, value)
		}
	}
	sort.Strings(values)
	return values
}

func sameStrings(got, want []string) bool {
	if len(got) != len(want) {
		return false
	}
	for i := range got {
		if got[i] != want[i] {
			return false
		}
	}
	return true
}
