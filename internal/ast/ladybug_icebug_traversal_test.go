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
	// Env-gated: the real-corpus timing is only meaningful with a populated store.
	// Without GRAPHIT_REAL_STORE the test exercises the same bounded 3-hop plan on
	// the fixture, so the timing lane still runs in CI.
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}
	query := "MATCH (caller)-[:CALLS*1..3]->(t) WHERE " +
		"(label(t) = 'Function' OR label(t) = 'Method') AND " +
		"t.uid IN ['internal/ast/ladybug.go::runQuery'] RETURN DISTINCT caller.uid"

	// The local store IS the bundle — "native" and "mounted" are both in-memory
	// catalogs over the same Parquets now. The probe's real value is the timing
	// of the bounded plan, which is why it env-gates on a populated corpus.
	mounted := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(storePath), IcebugDir: filepath.Join(filepath.Dir(storePath), "graph.icebug")})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open store graph: %v", err)
	}
	defer func() { _ = mounted.Close() }()
	start := time.Now()
	got, err := mounted.Query(context.Background(), query, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("mounted query: %v", err)
	}
	t.Logf("bounded 3-hop traversal over Icebug: rows=%d took=%s", len(got.Records), took)
	if len(got.Records) == 0 {
		t.Fatal("bounded traversal returned no rows")
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
	native := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(storePath), IcebugDir: filepath.Join(filepath.Dir(storePath), "graph.icebug")})
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

	mountedPath := filepath.Join(t.TempDir(), "store")
	if schema, err := bundleSchemaBytes(filepath.Dir(storePath)); err == nil {
		if err := MountIcebugGraph(ctx, mountedPath, string(schema), nil); err != nil {
			t.Fatalf("mount remote Icebug: %v", err)
		}
	} else {
		t.Fatalf("read store schema: %v", err)
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{StoreDir: mountedPath, IcebugDir: mountedPath})
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
	// The fixture is a store built straight from shards into an icebug bundle —
	// four Function rows, a CALLS CSR, schema.cypher and icebug.json — then
	// mounted in-memory. No Ladybug file DB ever exists; that is the whole model.
	entry := &parseCacheEntry{
		RelPath: "target.go", Language: "go",
		Entities: []cachedEntity{
			{Label: "Function", UID: "target", Name: "target", Path: "target.go", Line: 1, EndLine: 3},
			{Label: "Function", UID: "caller1", Name: "caller1", Path: "caller1.go", Line: 1, EndLine: 3},
			{Label: "Function", UID: "caller2", Name: "caller2", Path: "shared.go", Line: 1, EndLine: 3},
			{Label: "Function", UID: "caller3", Name: "caller3", Path: "shared.go", Line: 1, EndLine: 3},
		},
		Calls: []cachedCall{
			{CallerUID: "caller1", CalleeUID: "target", SourceType: "Function", Path: "caller1.go", Line: 3, Lang: "go"},
			{CallerUID: "caller2", CalleeUID: "caller1", SourceType: "Function", Path: "shared.go", Line: 3, Lang: "go"},
			{CallerUID: "caller3", CalleeUID: "caller2", SourceType: "Function", Path: "shared.go", Line: 3, Lang: "go"},
		},
	}
	cacheDir := t.TempDir()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()
	if err := cache.Store("target.go", "h-target", entry); err != nil {
		t.Fatalf("store shard: %v", err)
	}
	db := rebuildTestStore(t, cache, cacheDir)
	return db
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

func bundleSchema(t *testing.T, storeDir string) string {
	t.Helper()
	raw, err := bundleSchemaBytes(storeDir)
	if err != nil {
		t.Fatalf("read schema: %v", err)
	}
	return string(raw)
}

func bundleSchemaBytes(storeDir string) ([]byte, error) {
	for _, p := range []string{
		filepath.Join(storeDir, "schema.cypher"),
		filepath.Join(storeDir, "graph.icebug", "schema.cypher"),
	} {
		if raw, err := os.ReadFile(p); err == nil {
			return raw, nil
		}
	}
	return nil, os.ErrNotExist
}
