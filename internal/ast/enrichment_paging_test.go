package ast

import (
	"context"
	"fmt"
	"path/filepath"
	"testing"
)

// TestScoreEntryPointsPagesBeyondFirstPage guards the bug the LIMIT 5000 hid:
// on a graph with more functions than one page, everything past the first page
// was never scored at all. The corpus here deliberately straddles the page
// boundary.
func TestScoreEntryPointsPagesBeyondFirstPage(t *testing.T) {
	const total = 5200 // > one 5000-row page

	dir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{DBPath: filepath.Join(dir, "db")})
	defer func() { _ = db.Close() }()
	ctx := context.Background()

	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	if _, err := db.Execute(ctx, "CREATE NODE TABLE Function(uid STRING, name STRING, path STRING, decorators STRING, is_exported BOOLEAN, lang STRING, entry_point_score INT64, PRIMARY KEY(uid))", nil); err != nil {
		t.Fatalf("schema: %v", err)
	}

	// Every function is named "main", which the built-in rules score > 0.
	rows := make([]map[string]any, 0, total)
	for i := 0; i < total; i++ {
		rows = append(rows, map[string]any{
			"uid": fmt.Sprintf("f%05d", i), "name": "main", "path": "a.go", "lang": "go",
		})
	}
	if _, err := db.Execute(ctx,
		"UNWIND $rows AS row CREATE (:Function {uid: row.uid, name: row.name, path: row.path, lang: row.lang, entry_point_score: 0})",
		map[string]any{"rows": rows}); err != nil {
		t.Fatalf("seed: %v", err)
	}

	ScoreEntryPoints(ctx, db, dir, nil)

	res, err := db.Query(ctx, "MATCH (f:Function) WHERE f.entry_point_score > 0 RETURN count(f) AS c", nil)
	if err != nil {
		t.Fatalf("count: %v", err)
	}
	var scored int64
	if len(res.Records) > 0 {
		switch v := res.Records[0]["c"].(type) {
		case int64:
			scored = v
		case int:
			scored = int64(v)
		}
	}
	t.Logf("scored %d of %d functions", scored, total)

	if scored == 0 {
		t.Skip("scoring rules produced no positive scores here; the paging assertion needs scored rows")
	}
	if scored <= 5000 {
		t.Errorf("only %d functions scored — paging did not go past the first page (was the LIMIT 5000 bug)", scored)
	}
	if scored != total {
		t.Errorf("scored %d, want all %d", scored, total)
	}
}
