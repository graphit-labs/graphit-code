package ast

import (
	"context"
	"path/filepath"
	"testing"
)

func TestLadybugQueryPageHonoursOffsetLookaheadAndCypherLimit(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: root, IcebugDir: filepath.Join(root, "graph.icebug")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()
	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function"}}); err != nil {
		t.Fatal(err)
	}
	for _, q := range []string{
		`CREATE (:Function {uid:'a', name:'A'})`,
		`CREATE (:Function {uid:'b', name:'B'})`,
		`CREATE (:Function {uid:'c', name:'C'})`,
		`CREATE (:Function {uid:'d', name:'D'})`,
	} {
		if _, err := db.Execute(context.Background(), q, nil); err != nil {
			t.Fatal(err)
		}
	}

	page, err := db.QueryPage(context.Background(),
		`MATCH (n:Function) RETURN n.name AS name ORDER BY name`, nil, 1, 3)
	if err != nil {
		t.Fatal(err)
	}
	if got := queryNames(page.Records); len(got) != 3 || got[0] != "B" || got[1] != "C" || got[2] != "D" {
		t.Fatalf("offset plus look-ahead = %v, want [B C D]", got)
	}

	limited, err := db.QueryPage(context.Background(),
		`MATCH (n:Function) RETURN n.name AS name ORDER BY name LIMIT 2`, nil, 1, 3)
	if err != nil {
		t.Fatal(err)
	}
	if got := queryNames(limited.Records); len(got) != 1 || got[0] != "B" {
		t.Fatalf("Cypher LIMIT was not preserved: %v", got)
	}
}

func queryNames(records []QueryRecord) []string {
	out := make([]string, 0, len(records))
	for _, record := range records {
		if name, ok := record["name"].(string); ok {
			out = append(out, name)
		}
	}
	return out
}
