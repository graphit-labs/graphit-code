package ast

import (
	"context"
	"fmt"
	"path/filepath"
	"testing"
)

// sondaSchema mirrors what a grammar that does not declare Parameter and Field
// produces: those two get the minimal node table, without a path column, and can
// only be reached through their owner. That is the case the delete has to handle
// without a file path to match on.
// ParamOwnerLabels has to name Function explicitly: HAS_PARAMETER is built from
// the owner labels alone, and the fallback that used to guess `FROM Function`
// when they were empty is gone — it invented a rel table end for a node table
// the corpus never created.
var sondaSchema = SchemaInfo{
	Labels:           []string{"Function", "Struct", "Method"},
	CallerLabels:     []string{"Function", "Method"},
	ParamOwnerLabels: []string{"Function"},
	FieldOwnerLabels: []string{"Struct"},
	ContainsPairs:    [][2]string{{"Struct", "Method"}},
	HasParams:        true,
	HasFields:        true,
}

// seedSondaGraph builds a graph by hand rather than by indexing: the pipeline
// only picks up files whose grammar is installed, which a bare temp directory has
// none of. Every node here is what the writer would have produced for path.
func seedSondaGraph(t *testing.T, db *LadybugBackend, path, suffix string) {
	t.Helper()
	ctx := context.Background()

	stmts := []string{
		fmt.Sprintf(`CREATE (:File {path: '%s', name: 'veio%s'})`, path, suffix),
		fmt.Sprintf(`CREATE (:Function {uid: 'fn%s', name: 'Olivina%s', path: '%s'})`, suffix, suffix, path),
		fmt.Sprintf(`CREATE (:Struct {uid: 'st%s', name: 'Cianita%s', path: '%s'})`, suffix, suffix, path),
		fmt.Sprintf(`CREATE (:Method {uid: 'me%s', name: 'Peridotito%s', path: '%s'})`, suffix, suffix, path),
		fmt.Sprintf(`CREATE (:Parameter {uid: 'pa%s', name: 'basalto%s'})`, suffix, suffix),
		fmt.Sprintf(`CREATE (:Field {uid: 'fi%s', name: 'estaurolita%s'})`, suffix, suffix),

		fmt.Sprintf(`MATCH (f:File {path: '%s'}), (n:Function {uid: 'fn%s'}) CREATE (f)-[:CONTAINS]->(n)`, path, suffix),
		fmt.Sprintf(`MATCH (f:File {path: '%s'}), (n:Struct {uid: 'st%s'}) CREATE (f)-[:CONTAINS]->(n)`, path, suffix),
		fmt.Sprintf(`MATCH (s:Struct {uid: 'st%s'}), (m:Method {uid: 'me%s'}) CREATE (s)-[:CONTAINS]->(m)`, suffix, suffix),
		fmt.Sprintf(`MATCH (n:Function {uid: 'fn%s'}), (p:Parameter {uid: 'pa%s'}) CREATE (n)-[:HAS_PARAMETER]->(p)`, suffix, suffix),
		fmt.Sprintf(`MATCH (s:Struct {uid: 'st%s'}), (x:Field {uid: 'fi%s'}) CREATE (s)-[:HAS_FIELD]->(x)`, suffix, suffix),
	}
	for _, q := range stmts {
		if _, err := db.Execute(ctx, q, nil); err != nil {
			t.Fatalf("seed %q: %v", q, err)
		}
	}
}

// newSondaGraph returns a connected backend holding the seeded files.
func newSondaGraph(t *testing.T, root string, files map[string]string) *LadybugBackend {
	t.Helper()

	db := NewLadybugDB(LadybugConfig{DBPath: filepath.Join(root, "db")})
	t.Cleanup(func() { _ = db.Close() })

	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	if err := db.initSchemaForLabels(sondaSchema); err != nil {
		t.Fatalf("schema: %v", err)
	}
	for path, suffix := range files {
		seedSondaGraph(t, db, path, suffix)
	}
	return db
}

// countNodes totals every node in the graph, so nothing hides in a label the
// test did not think to name.
func countNodes(t *testing.T, db GraphDB) map[string]int64 {
	t.Helper()
	ctx := context.Background()

	counts := make(map[string]int64)
	for _, label := range ActiveNodeLabels(ctx, db) {
		res, err := db.Query(ctx, fmt.Sprintf("MATCH (n:`%s`) RETURN count(n) AS c", label), nil)
		if err != nil {
			t.Fatalf("count %s: %v", label, err)
		}
		if len(res.Records) == 0 {
			continue
		}
		var c int64
		switch v := res.Records[0]["c"].(type) {
		case int64:
			c = v
		case int:
			c = int64(v)
		}
		if c > 0 {
			counts[label] = c
		}
	}
	return counts
}

// TestDeleteRepositoryEmptiesTheGraph is the regression test for the stub that
// returned nil without deleting anything, which is why a forced reindex left the
// entities of deleted files behind until someone ran a full reset.
func TestDeleteRepositoryEmptiesTheGraph(t *testing.T) {
	root := t.TempDir()
	db := newSondaGraph(t, root, map[string]string{"sonda.go": "A"})

	before := countNodes(t, db)
	if len(before) == 0 {
		t.Fatal("nothing was seeded, so the deletion would prove nothing")
	}

	w := NewGraphWriter(db, root, true)
	if err := w.DeleteRepository(context.Background(), root); err != nil {
		t.Fatalf("DeleteRepository() error: %v", err)
	}

	if after := countNodes(t, db); len(after) > 0 {
		t.Errorf("DeleteRepository() left %v behind; seeded %v", after, before)
	}
}

// TestDeleteRepositoryScopesToSubdirectory covers the partial delete, where the
// orphan sweep for pathless Parameter and Field nodes could easily overreach:
// the untouched file's parameter still has an owner and must survive.
func TestDeleteRepositoryScopesToSubdirectory(t *testing.T) {
	root := t.TempDir()
	db := newSondaGraph(t, root, map[string]string{
		"sonda.go":          "A",
		"pedreira/veio.go":  "B",
		"pedreira/xisto.go": "C",
	})

	w := NewGraphWriter(db, root, true)
	if err := w.DeleteRepository(context.Background(), filepath.Join(root, "pedreira")); err != nil {
		t.Fatalf("DeleteRepository() error: %v", err)
	}

	after := countNodes(t, db)
	want := map[string]int64{
		"File": 1, "Function": 1, "Struct": 1, "Method": 1, "Parameter": 1, "Field": 1,
	}
	for label, n := range want {
		if after[label] != n {
			t.Errorf("%s count = %d; want %d (only sonda.go should survive)", label, after[label], n)
		}
	}
	for label, n := range after {
		if want[label] == 0 {
			t.Errorf("unexpected %s nodes left: %d", label, n)
		}
	}
}

// TestDeleteRepositoryIgnoresPathsOutsideTheRoot keeps the delete scoped: a path
// the writer does not own must not empty the graph.
func TestDeleteRepositoryIgnoresPathsOutsideTheRoot(t *testing.T) {
	root := t.TempDir()
	db := newSondaGraph(t, root, map[string]string{"sonda.go": "A"})

	before := countNodes(t, db)
	if len(before) == 0 {
		t.Fatal("nothing was seeded, so the deletion would prove nothing")
	}

	w := NewGraphWriter(db, root, true)
	if err := w.DeleteRepository(context.Background(), t.TempDir()); err != nil {
		t.Fatalf("DeleteRepository() error: %v", err)
	}

	after := countNodes(t, db)
	for label, want := range before {
		if after[label] != want {
			t.Errorf("%s count = %d; want %d unchanged", label, after[label], want)
		}
	}
	if len(after) != len(before) {
		t.Errorf("label set changed: %v -> %v", before, after)
	}
}
