package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

func buildCanonicalFixture(t *testing.T) (mounted *LadybugBackend) {
	t.Helper()
	names := []string{"a", "b", "c", "d", "e", "f"}
	var ents []cachedEntity
	var calls []cachedCall
	for _, n := range names {
		ents = append(ents, cachedEntity{Label: "Function", UID: "fn_" + n, Name: n, Path: "f.go", Line: 1, EndLine: 2})
	}
	for i := 0; i+1 < len(names); i++ {
		calls = append(calls, cachedCall{CallerUID: "fn_" + names[i], CalleeUID: "fn_" + names[i+1], SourceType: "Function", Path: "f.go", Line: 1, Lang: "go"})
	}
	entry := &parseCacheEntry{RelPath: "f.go", Language: "go", Entities: ents, Calls: calls}
	ctx := context.Background()
	_ = ctx

	bundle := filepath.Join(t.TempDir(), "bundle")
	ri := newRebuildIndex(map[string]*parseCacheEntry{"f.go": entry}, targetRulesFor(""))
	if _, err := ExportDirectFromRebuildIndex(ri, bundle, bundle); err != nil {
		t.Fatalf("canonical export: %v", err)
	}

	schemaRaw, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("schema: %v", err)
	}
	mountDir := t.TempDir()
	if err := MountIcebugGraph(ctx, mountDir, string(schemaRaw), nil); err != nil {
		t.Fatalf("mount: %v", err)
	}
	manifestRaw, err := os.ReadFile(filepath.Join(bundle, ladybug.IcebugManifestFile))
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(mountDir, ladybug.IcebugManifestFile), manifestRaw, 0o644); err != nil {
		t.Fatalf("stage manifest: %v", err)
	}

	mounted = NewLadybugDBReadOnly(LadybugConfig{StoreDir: mountDir, IcebugDir: mountDir})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	t.Cleanup(func() { _ = mounted.Close() })
	return mounted
}

func TestMountedCanonicalUnboundedTraversalMatchesNative(t *testing.T) {
	mounted := buildCanonicalFixture(t)

	publicQ := "MATCH (caller)-[:CALLS*]->(t) WHERE t.uid IN ['fn_f'] RETURN DISTINCT caller.uid AS uid"
	got, err := mounted.Query(context.Background(), publicQ, nil)
	if err != nil {
		t.Fatalf("canonical planner: %v", err)
	}
	gotUIDs := recordStrings(got, "uid")
	if len(gotUIDs) != 5 {
		t.Fatalf("uncapped traversal returned %v, want the full 5-hop chain a..e", gotUIDs)
	}
	seen := map[string]bool{}
	for _, u := range gotUIDs {
		seen[u] = true
	}
	for _, n := range []string{"a", "b", "c", "d", "e"} {
		if !seen["fn_"+n] {
			t.Fatalf("missing caller fn_%s (got %v)", n, gotUIDs)
		}
	}
}

func TestMountedCanonicalCountDistinct(t *testing.T) {
	mounted := buildCanonicalFixture(t)
	res, err := mounted.Query(context.Background(),
		"MATCH (caller)-[:CALLS*1..3]->(t) WHERE t.uid IN ['fn_f'] RETURN count(DISTINCT caller.uid)", nil)
	if err != nil {
		t.Fatalf("count distinct: %v", err)
	}
	var got int64
	for _, r := range res.Records {
		for _, v := range r {
			got = ladybug.Int64(v)
		}
	}
	if got != 3 {
		t.Fatalf("count(DISTINCT caller.uid)=%d want 3", got)
	}
}

func TestMountedCanonicalUnsupportedProjectionFailsClosed(t *testing.T) {
	mounted := buildCanonicalFixture(t)
	_, err := mounted.Query(context.Background(),
		"MATCH (caller)-[:CALLS*1..3]->(t) WHERE t.uid IN ['fn_f'] RETURN collect(caller.uid)", nil)
	if err == nil || !stringsContains(err.Error(), "must be DISTINCT") {
		t.Fatalf("want a fail-closed refusal naming its rule, got %v", err)
	}
}

func stringsContains(s, sub string) bool {
	return len(sub) == 0 || (len(s) >= len(sub) && indexOf(s, sub) >= 0)
}
func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}

func TestSanitizeCanonicalUIDEquality(t *testing.T) {
	in := "MATCH (f:Function {name:'x'}) WHERE f.uid = 'fn_1' RETURN f.name"
	want := "MATCH (f:Function {name:'x'}) WHERE f.uid IN ['fn_1'] RETURN f.name"
	if got := sanitizeCanonicalUIDEquality(in); got != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
	lit := "MATCH (n) WHERE n.name = 'f.uid = x' RETURN n"
	if got := sanitizeCanonicalUIDEquality(lit); got != lit {
		t.Fatalf("string literal mangled:\n%s", got)
	}
}

func TestMountedCanonicalBarePatternIsExactlyOneHop(t *testing.T) {
	mounted := buildCanonicalFixture(t)
	q := "MATCH (c)-[:CALLS]->(t) WHERE t.uid IN ['fn_b'] RETURN DISTINCT c.uid AS uid"
	got, err := mounted.Query(context.Background(), q, nil)
	if err != nil {
		t.Fatalf("canonical bare pattern: %v", err)
	}
	gotUIDs := recordStrings(got, "uid")
	if len(gotUIDs) != 1 || gotUIDs[0] != "fn_a" {
		t.Fatalf("bare single-hop returned %v; want only the direct caller fn_a", gotUIDs)
	}
}

func TestCanonicalMembersExpandSmallestFirst(t *testing.T) {
	g := &ladybug.CanonicalRelGroup{Type: "CALLS", Members: []ladybug.CanonicalMember{
		{From: "Function", To: "Function", Table: "big", Rows: 46201},
		{From: "Method", To: "Method", Table: "small", Rows: 1222},
		{From: "Function", To: "Method", Table: "mid", Rows: 3553},
	}}
	m := &ladybug.CanonicalManifest{NodeTables: []ladybug.CanonicalNodeTable{
		{Label: "Function", Columns: []ladybug.Field{{Name: "uid"}}},
		{Label: "Method", Columns: []ladybug.Field{{Name: "uid"}}},
	}}
	out := canonicalUIDMembers(m, g, false, false)
	if len(out) != 3 || out[0].Table != "small" || out[1].Table != "mid" || out[2].Table != "big" {
		t.Fatalf("expansion order = %+v, want small,mid,big", out)
	}
}
