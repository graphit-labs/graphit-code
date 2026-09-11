package ast

import (
	"context"
	"sort"
	"testing"
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

func mountedIcebugTraversalFixture(t *testing.T) *LadybugBackend {
	t.Helper()
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
