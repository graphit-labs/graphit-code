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
	"github.com/graphit-labs/graphit-code/internal/store"
)

const (
	e2eSettle = 25 * time.Second
	e2ePoll   = 200 * time.Millisecond
)

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

func searchFinds(t *testing.T, projectDir, query, want string) bool {
	t.Helper()
	idxPath := store.ASTProjectDir(projectDir)
	if _, err := os.Stat(filepath.Join(idxPath, "graph.icebug", "schema.cypher")); err != nil {
		return false
	}
	si, err := ast.OpenSearchIndex(context.Background(), idxPath)
	if err != nil {
		time.Sleep(150 * time.Millisecond)
		si, err = ast.OpenSearchIndex(context.Background(), idxPath)
	}
	if err != nil {
		return false
	}
	defer func() { _ = si.Close() }()

	res, err := si.Search(context.Background(), query, 25)
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

	existing := filepath.Join(projectDir, "existing.sql")
	if err := os.WriteFile(existing, []byte(plsqlFunction("FUNCAO_EXISTENTE")), 0o644); err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))

	mod.reindexAST(ctx, nil, nil, nil)
	if !searchFinds(t, projectDir, "FUNCAO_EXISTENTE", "FUNCAO_EXISTENTE") {
		t.Fatal("seeding the index did not index existing.sql; the daemon cannot be judged from here")
	}

	errCh := make(chan error, 1)
	go func() { errCh <- mod.Start(ctx) }()

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
