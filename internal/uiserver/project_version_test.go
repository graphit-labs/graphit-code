package uiserver

import (
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestRequestsResolveStoresOfTheRequestedVersion(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	project := t.TempDir()
	if err := os.WriteFile(filepath.Join(project, brand.LockFileName()),
		[]byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	var seen string
	handler := withProjectVersion(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = store.KnowledgeProjectDir(project)
	}), project)
	root := store.KnowledgeProjectRoot(project)

	for _, c := range []struct{ query, wantVersion string }{
		{"project_dir=" + url.QueryEscape(project) + "&project_version=" + url.QueryEscape("feature/x"), "feature/x"},
		{"project_dir=" + url.QueryEscape(project), "main"},
		{"", "main"}, // the server's default project
	} {
		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/memories?"+c.query, nil))
		if rec.Code != http.StatusOK {
			t.Fatalf("%q: status %d %s", c.query, rec.Code, rec.Body.String())
		}
		if got := rec.Header().Get(ProjectVersionHeader); got != c.wantVersion {
			t.Errorf("%q: %s = %q, want %q", c.query, ProjectVersionHeader, got, c.wantVersion)
		}
		if want := store.VersionStoreDir(root, c.wantVersion); seen != want {
			t.Errorf("%q: resolved %q, want %q", c.query, seen, want)
		}
	}

	rec := httptest.NewRecorder()
	withProjectVersion(http.NotFoundHandler(), "").ServeHTTP(rec,
		httptest.NewRequest(http.MethodGet, "/api/memories?project_version=x", nil))
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("a version without a project must be rejected, got %d", rec.Code)
	}
}
