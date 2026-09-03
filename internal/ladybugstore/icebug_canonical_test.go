package ladybugstore

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestIcebugCanonicalRoundTrip(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)

	out := t.TempDir()
	man, err := ExportIcebugCanonical(src, out, IcebugOptions{StorageURI: out})
	if err != nil {
		t.Fatalf("ExportIcebugCanonical: %v", err)
	}

	if man.Format != "icebug-canonical" || man.Version != CanonicalManifestVersion || !man.Finished {
		t.Fatalf("manifest header = %+v", man)
	}
	if got := len(man.NodeTables); got != 3 {
		t.Fatalf("node tables = %d, want 3 (File, Function, Comment)", got)
	}

	var calls *CanonicalRelGroup
	var contains *CanonicalRelGroup
	for i := range man.RelGroups {
		switch man.RelGroups[i].Type {
		case "CALLS":
			calls = &man.RelGroups[i]
		case "CONTAINS":
			contains = &man.RelGroups[i]
		}
	}
	if calls == nil || contains == nil {
		t.Fatalf("rel groups missing: %+v", man.RelGroups)
	}
	if len(contains.Members) != 2 {
		t.Fatalf("CONTAINS members = %d, want one per real pair", len(contains.Members))
	}
	if len(calls.Members) != 1 || calls.Members[0].From != "Function" || calls.Members[0].To != "Function" {
		t.Fatalf("CALLS members = %+v", calls.Members)
	}
	if len(calls.ReverseMembers) != 1 {
		t.Fatalf("CALLS reverse mirror missing")
	}

	mounted := mountIcebug(t, out)

	f2f, err := mounted.Query(
		"MATCH (f:File)-[:contains__file_function]->(n:Function) RETURN f.path AS p, n.uid AS u ORDER BY p, u", nil)
	if err != nil {
		t.Fatalf("file->function member: %v", err)
	}
	if len(f2f) != 3 {
		t.Fatalf("file->function rows=%d want 3", len(f2f))
	}
	f2c, err := mounted.Query(
		"MATCH (f:File)-[:contains__file_comment]->(c:Comment) RETURN f.path AS p, c.uid AS u", nil)
	if err != nil {
		t.Fatalf("file->comment member: %v", err)
	}
	if len(f2c) != 1 || Str(f2c[0]["u"]) != "c1" {
		t.Fatalf("file->comment rows=%+v want exactly c1", f2c)
	}

	total, err := mounted.Query("MATCH ()-[r:calls__function_function]->() RETURN count(r) AS c", nil)
	if err != nil {
		t.Fatalf("calls total: %v", err)
	}
	if Int64(total[0]["c"]) != 3 {
		t.Fatalf("calls total=%d want 3 (two hops plus the recursive self-loop)", total[0]["c"])
	}
	loop, err := mounted.Query(
		"MATCH (x:Function)-[:calls__function_function]->(x) RETURN x.uid AS u", nil)
	if err != nil {
		t.Fatalf("self loop: %v", err)
	}
	if len(loop) != 1 || Str(loop[0]["u"]) != "fn2" {
		t.Fatalf("self loop rows=%+v want exactly fn2", loop)
	}

	rev, err := mounted.Query(
		"MATCH (a:Function)-[:calls__function_function_reverse]->(b:Function) "+
			"RETURN a.uid AS from_u, b.uid AS to_u ORDER BY from_u", nil)
	if err != nil {
		t.Fatalf("mirror: %v", err)
	}
	if len(rev) != 2 {
		t.Fatalf("mirror rows=%d want 2 (self-loop excluded)", len(rev))
	}
	for _, r := range rev {
		fromU, toU := Str(r["from_u"]), Str(r["to_u"])
		back, qerr := mounted.Query(
			"MATCH (a:Function)-[:calls__function_function]->(b:Function) "+
				"WHERE a.uid IN [$a] AND b.uid IN [$b] RETURN count(*) AS c",
			map[string]any{"a": toU, "b": fromU})
		if qerr != nil {
			t.Fatalf("verify forward edge %v: %v", r, qerr)
		}
		if Int64(back[0]["c"]) != 1 {
			t.Fatalf("mirror row %v has no forward counterpart — invented adjacency", r)
		}
	}

	raw, err := os.ReadFile(filepath.Join(out, "schema.cypher"))
	if err != nil {
		t.Fatalf("schema: %v", err)
	}
	schema := string(raw)
	for _, want := range []string{
		"FROM `File` TO `Function`",
		"FROM `File` TO `Comment`",
		"FROM `Function` TO `Function`",
	} {
		if !strings.Contains(schema, want) {
			t.Fatalf("schema.cypher lacks %q:\n%s", want, schema)
		}
	}
	if strings.Contains(schema, "`Entity`") {
		t.Fatal("canonical schema must not declare a folded Entity table:\n" + schema)
	}
}

func TestIcebugCanonicalReverseOptOut(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)
	out := t.TempDir()
	man, err := ExportIcebugCanonical(src, out, IcebugOptions{
		StorageURI:          out,
		DisableReverseEdges: true,
	})
	if err != nil {
		t.Fatalf("export: %v", err)
	}
	if man.Reverse {
		t.Fatal("manifest claims reverses despite opt-out")
	}
	for _, g := range man.RelGroups {
		if len(g.ReverseMembers) != 0 {
			t.Fatalf("type %s carried reverse members despite opt-out", g.Type)
		}
	}
	mounted := mountIcebug(t, out)
	if _, err := mounted.Query(
		"MATCH ()-[r:calls__function_function_reverse]->() RETURN count(r) AS c", nil); err == nil {
		t.Fatal("reverse table mounted despite opt-out")
	}
}

func TestCanonicalMemberNameSanitizesAndEncodesPair(t *testing.T) {
	got := canonicalMemberName("READS_FIELD", "Function", "QueryRecord")
	if got != "reads_field__function_queryrecord" {
		t.Fatalf("member name = %q", got)
	}
	got = canonicalMemberName("HAS-PARAM", "Node.Type", "Param")
	if got != "has_param__node_type_param" {
		t.Fatalf("sanitized member name = %q", got)
	}
}

func TestIcebugCanonicalPKEqualityReturnsZero(t *testing.T) {
	src, _ := buildHeterogeneousStore(t)
	out := t.TempDir()
	if _, err := ExportIcebugCanonical(src, out, IcebugOptions{StorageURI: out}); err != nil {
		t.Fatalf("export: %v", err)
	}
	mounted := mountIcebug(t, out)

	eq, err := mounted.Query(
		"MATCH (f:Function) WHERE f.uid = 'fn1' RETURN count(*) AS c", nil)
	if err != nil {
		t.Fatalf("equality probe: %v", err)
	}
	in, err := mounted.Query(
		"MATCH (f:Function) WHERE f.uid IN ['fn1'] RETURN count(*) AS c", nil)
	if err != nil {
		t.Fatalf("in-list probe: %v", err)
	}
	if Int64(eq[0]["c"]) != 0 || Int64(in[0]["c"]) != 1 {
		t.Fatalf("pk equality=%d in-list=%d; expected the documented quirk (0 and 1)",
			eq[0]["c"], in[0]["c"])
	}
}
