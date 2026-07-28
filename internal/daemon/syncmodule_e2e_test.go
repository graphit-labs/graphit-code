package daemon

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

// End-to-end exercise of the daemon's sync module, which until now had never been run the
// way it runs in production.
//
// Everything below the module was covered in isolation — the watcher has its own tests, the
// pipeline has its own, the search index has its own — but the wiring between them had not
// been: a real write to a real file, delivered by the OS to fswatch, debounced, classified,
// and turned into a scoped reindex whose result is visible to search. That chain is where
// the interesting failures live, and it is the one gap where production code had never
// executed end to end.
//
// The test drives the module through its public Start, not through handleBatch, so the
// watcher and the debounce are part of what is verified.

const (
	// The module debounces for 1s and caps at 5s; reindexing a tiny project is fast, so
	// this budget is dominated by the debounce.
	e2eSettle = 25 * time.Second
	e2ePoll   = 200 * time.Millisecond
)

// plsqlFunction returns a small PL/SQL unit with a distinctive function name.
func plsqlFunction(name string) string {
	return "CREATE OR REPLACE FUNCTION " + name + `
 (P_ID_ENTRADA IN NUMBER)
 RETURN NUMBER
IS
  D_RESULTADO NUMBER;
BEGIN
  D_RESULTADO := P_ID_ENTRADA;
  RETURN D_RESULTADO;
END;
`
}

// searchFinds reports whether the project's search index currently returns an entity with
// the given name. It opens the index fresh on every call: the daemon writes it from another
// goroutine, and holding a handle open across a rebuild would read a stale one.
func searchFinds(t *testing.T, projectDir, query, want string) bool {
	t.Helper()
	idxPath := filepath.Join(projectDir, brand.DotDir(), "ast", "project", "ladybugdb") +
		ast.SearchIndexSuffix
	if _, err := os.Stat(idxPath); err != nil {
		return false
	}
	si, err := ast.OpenSearchIndex(idxPath)
	if err != nil {
		return false
	}
	defer func() { _ = si.Close() }()

	res, err := si.Search(query, 25)
	if err != nil {
		return false
	}
	for _, r := range res {
		if strings.EqualFold(r.Name, want) {
			return true
		}
	}
	return false
}

// waitFor polls until cond holds or the budget runs out, reporting how long it took.
func waitFor(t *testing.T, what string, cond func() bool) bool {
	t.Helper()
	start := time.Now()
	deadline := start.Add(e2eSettle)
	for time.Now().Before(deadline) {
		if cond() {
			t.Logf("  %s after %s", what, time.Since(start).Round(100*time.Millisecond))
			return true
		}
		time.Sleep(e2ePoll)
	}
	t.Logf("  %s did NOT happen within %s", what, e2eSettle)
	return false
}

func TestSyncModuleEndToEnd(t *testing.T) {
	if testing.Short() {
		t.Skip("starts a filesystem watcher and runs the indexing pipeline")
	}

	projectDir := t.TempDir()

	// Precondition: this machine can extract entities from the files the test writes. The
	// query definitions live in the runtime directory rather than in the binary, so a
	// machine without them would index nothing and every assertion below would fail for a
	// reason that has nothing to do with the daemon.
	probe := filepath.Join(projectDir, "probe.sql")
	if err := os.WriteFile(probe, []byte(plsqlFunction("PROBE_FUNCTION")), 0o644); err != nil {
		t.Fatal(err)
	}
	pf, err := ast.NewCompositeParser(projectDir, nil).Parse(probe, false, ast.ParseOptions{})
	if err != nil || pf == nil || pf.EntityCount() == 0 {
		t.Skipf("this machine extracts no entities from PL/SQL (err=%v) — the runtime query "+
			"files are probably missing; the daemon path cannot be judged from here", err)
	}
	if err := os.Remove(probe); err != nil {
		t.Fatal(err)
	}

	// A file that exists before the watcher starts, so later assertions can tell
	// "untouched" from "lost".
	existing := filepath.Join(projectDir, "existing.sql")
	if err := os.WriteFile(existing, []byte(plsqlFunction("FUNCAO_EXISTENTE")), 0o644); err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	mod := NewSyncModule(projectDir, filepath.Join(projectDir, brand.DotDir(), "ast", "project"))

	// Seed the index the way production does. The daemon never scans a project it
	// adopts — it only applies changes from the moment it starts watching — so an
	// index exists because `ast index` built one. Starting the module against an
	// empty project would make every "is the rest of the index still there?"
	// assertion below vacuously true.
	mod.reindexAST(ctx, nil, nil, nil)
	if !searchFinds(t, projectDir, "FUNCAO_EXISTENTE", "FUNCAO_EXISTENTE") {
		t.Fatal("seeding the index did not index existing.sql; the daemon cannot be judged from here")
	}

	errCh := make(chan error, 1)
	go func() { errCh <- mod.Start(ctx) }()

	// Give the watcher a moment to arm before generating events it must not miss.
	time.Sleep(500 * time.Millisecond)
	select {
	case err := <-errCh:
		t.Fatalf("sync module exited immediately: %v", err)
	default:
	}

	t.Log("1. create a file")
	created := filepath.Join(projectDir, "criada.sql")
	if err := os.WriteFile(created, []byte(plsqlFunction("FUNCAO_CRIADA")), 0o644); err != nil {
		t.Fatal(err)
	}
	if !waitFor(t, "FUNCAO_CRIADA is searchable", func() bool {
		return searchFinds(t, projectDir, "FUNCAO_CRIADA", "FUNCAO_CRIADA")
	}) {
		t.Error("a file created while the daemon was watching never reached the search index")
	}

	// The pre-existing file must also be indexed: a scoped reindex that only knows about
	// the changed path must not drop everything else.
	if !searchFinds(t, projectDir, "FUNCAO_EXISTENTE", "FUNCAO_EXISTENTE") {
		t.Error("the file that existed before the first reindex is missing from the index — " +
			"a scoped reindex dropped content it was not asked to touch")
	}

	t.Log("2. edit that file, renaming its function")
	if err := os.WriteFile(created, []byte(plsqlFunction("FUNCAO_RENOMEADA")), 0o644); err != nil {
		t.Fatal(err)
	}
	if !waitFor(t, "FUNCAO_RENOMEADA is searchable", func() bool {
		return searchFinds(t, projectDir, "FUNCAO_RENOMEADA", "FUNCAO_RENOMEADA")
	}) {
		t.Error("an edit never reached the search index")
	}
	if !waitFor(t, "FUNCAO_CRIADA is gone", func() bool {
		return !searchFinds(t, projectDir, "FUNCAO_CRIADA", "FUNCAO_CRIADA")
	}) {
		t.Error("the old name survived the edit — stale rows are not being removed")
	}

	t.Log("3. delete the file")
	if err := os.Remove(created); err != nil {
		t.Fatal(err)
	}
	if !waitFor(t, "FUNCAO_RENOMEADA is gone", func() bool {
		return !searchFinds(t, projectDir, "FUNCAO_RENOMEADA", "FUNCAO_RENOMEADA")
	}) {
		t.Error("a deleted file's entities are still searchable")
	}
	if !searchFinds(t, projectDir, "FUNCAO_EXISTENTE", "FUNCAO_EXISTENTE") {
		t.Error("deleting one file removed another file's entities")
	}

	t.Log("4. a file the indexer has no parser for must not trigger anything harmful")
	if err := os.WriteFile(filepath.Join(projectDir, "notas.xyz"), []byte("nada"), 0o644); err != nil {
		t.Fatal(err)
	}
	time.Sleep(2 * time.Second)
	if !searchFinds(t, projectDir, "FUNCAO_EXISTENTE", "FUNCAO_EXISTENTE") {
		t.Error("writing an unsupported file damaged the index")
	}

	cancel()
	select {
	case err := <-errCh:
		if err != nil && !errors.Is(err, context.Canceled) {
			t.Errorf("sync module exited with %v, want context.Canceled", err)
		}
	case <-time.After(10 * time.Second):
		t.Error("sync module did not stop within 10s of its context being cancelled")
	}
}
