package ast

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// writeProjectSearchIndex lays out a project the way the UI server expects to find
// one — its global store, with the search index beside the graph — and puts one
// file's text in that index. Returns the project root.
func writeProjectSearchIndex(t *testing.T, relPath, src string) string {
	t.Helper()

	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	root := t.TempDir()
	storeDir := store.ASTProjectDir(root)
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		t.Fatalf("mkdir store: %v", err)
	}

	idx, err := OpenSearchIndex(context.Background(), store.ASTProjectDBPath(root))
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	putFileRow(t, idx, relPath, src)
	// Closed before returning: the handler opens the same store read-only, and the
	// engine allows only one writer.
	if err := idx.Close(); err != nil {
		t.Fatalf("close search index: %v", err)
	}
	return root
}

// serverRootedAt builds a Server for a project without ever opening its graph:
// LadybugBackend connects lazily, so DBPath() answers from configuration alone.
func serverRootedAt(t *testing.T, root string) *Server {
	t.Helper()

	db := NewLadybugDBReadOnly(LadybugConfig{
		DBPath:   store.ASTProjectDBPath(root),
		ReadOnly: true,
	})
	srv, err := NewServerOnPort(db, root, 0)
	if err != nil {
		t.Fatalf("NewServerOnPort: %v", err)
	}
	return srv
}

func getFile(t *testing.T, srv *Server, query string) *httptest.ResponseRecorder {
	t.Helper()
	rec := httptest.NewRecorder()
	srv.handleFile(rec, httptest.NewRequest(http.MethodGet, "/api/file?"+query, nil))
	return rec
}

// TestHandleFileServesTheProjectTheRequestNames is the regression for a 404 on
// "view source" that only appeared once you switched projects in the UI.
//
// Every explorer call carries project_dir, and /api/graph honours it — so the graph
// on screen was the other project's. /api/file did not: it asked storePathFor, which
// only knows the project the server was started in, and answered "File source not
// found" for a file that is indexed, just not in the store it looked at.
func TestHandleFileServesTheProjectTheRequestNames(t *testing.T) {
	const otherRel = "internal/ast/ladybug_gc_pressure_test.go"
	const otherSrc = "package ast\n\n// the file the user clicked\n"

	ownRoot := writeProjectSearchIndex(t, "cmd/main.go", "package main\n")
	otherRoot := writeProjectSearchIndex(t, otherRel, otherSrc)

	srv := serverRootedAt(t, ownRoot)

	rec := getFile(t, srv, "path="+otherRel+"&project_dir="+otherRoot)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d (%s), want 200 — the request named a project whose index holds this file",
			rec.Code, rec.Body.String())
	}

	var got map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatalf("decode body: %v", err)
	}
	if got["content"] != otherSrc {
		t.Errorf("content = %q, want %q", got["content"], otherSrc)
	}
}

// TestHandleFileServesItsOwnProjectWithoutProjectDir keeps the default path honest:
// no project_dir, and no project_dir equal to the server's own root, must both go on
// reading the store this server was started in.
func TestHandleFileServesItsOwnProjectWithoutProjectDir(t *testing.T) {
	const rel = "cmd/main.go"
	const src = "package main\n\nfunc main() {}\n"

	root := writeProjectSearchIndex(t, rel, src)
	srv := serverRootedAt(t, root)

	for name, query := range map[string]string{
		"omitted":      "path=" + rel,
		"own root":     "path=" + rel + "&project_dir=" + root,
		"own project":  "path=" + rel + "&context=__project__",
		"empty string": "path=" + rel + "&project_dir=",
	} {
		t.Run(name, func(t *testing.T) {
			rec := getFile(t, srv, query)
			if rec.Code != http.StatusOK {
				t.Fatalf("status = %d (%s), want 200", rec.Code, rec.Body.String())
			}
			var got map[string]string
			if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
				t.Fatalf("decode body: %v", err)
			}
			if got["content"] != src {
				t.Errorf("content = %q, want %q", got["content"], src)
			}
		})
	}
}

// TestHandleFileStillRefusesWhatIsNotIndexed guards the other direction: resolving a
// different project must not turn a genuine miss into a hit somewhere else.
func TestHandleFileStillRefusesWhatIsNotIndexed(t *testing.T) {
	ownRoot := writeProjectSearchIndex(t, "cmd/main.go", "package main\n")
	otherRoot := writeProjectSearchIndex(t, "internal/other/file.go", "package other\n")

	srv := serverRootedAt(t, ownRoot)

	// cmd/main.go exists in THIS server's project, and is asked for in the other one.
	rec := getFile(t, srv, "path=cmd/main.go&project_dir="+otherRoot)
	if rec.Code != http.StatusNotFound {
		t.Fatalf("status = %d (%s), want 404 — that project's index does not hold this file",
			rec.Code, rec.Body.String())
	}

	rec = getFile(t, srv, "project_dir="+ownRoot)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400 for a missing path param", rec.Code)
	}
}

// TestStorePathForRequestResolvesRelativeStoresAgainstTheRoot covers `ui --repo
// <path>` started from somewhere else. Every resolver now returns an absolute path,
// so this is the guard for a backend built by hand with a relative one: left alone it
// would be looked up under whatever directory the server happened to be launched
// from.
func TestStorePathForRequestResolvesRelativeStoresAgainstTheRoot(t *testing.T) {
	root := t.TempDir()
	relStore := filepath.Join(".graphit", "ast", "project", "ladybugdb")

	srv, err := NewServerOnPort(NewLadybugDBReadOnly(LadybugConfig{DBPath: relStore}), root, 0)
	if err != nil {
		t.Fatalf("NewServerOnPort: %v", err)
	}

	got := srv.storePathForRequest(httptest.NewRequest(http.MethodGet, "/api/file?path=x.go", nil))
	if want := filepath.Join(root, relStore); got != want {
		t.Errorf("storePathForRequest = %q, want %q", got, want)
	}
}
