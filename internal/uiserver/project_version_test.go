package uiserver

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
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

func TestGlobalASTPOSTAcquiresSelectedVersion(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	project := t.TempDir()
	for _, path := range []string{"/api/query", "/api/generate-cypher"} {
		t.Run(path, func(t *testing.T) {
			entered, unblock, finished := make(chan string, 2), make(chan struct{}), make(chan *httptest.ResponseRecorder, 2)
			first := true
			h := withProjectVersion(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var body map[string]any
				if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body["project_dir"] != project {
					t.Error("middleware lost request body")
				}
				entered <- projectversion.Current(project).Version
				if first {
					first = false
					<-unblock
				}
			}), brand.GlobalDir())
			request := func(version string) {
				body, _ := json.Marshal(map[string]string{"cypher": "RETURN 1", "query": "files", "project_dir": project, "project_version": version})
				rec := httptest.NewRecorder()
				h.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, path, strings.NewReader(string(body))))
				finished <- rec
			}
			go request("feature/x")
			if got := <-entered; got != "feature/x" {
				t.Fatalf("POST did not select requested version: %q", got)
			}
			go request("main")
			select {
			case got := <-entered:
				t.Errorf("concurrent request bypassed version lease: %s", got)
			case <-time.After(20 * time.Millisecond):
			}
			close(unblock)
			if got := <-entered; got != "main" {
				t.Errorf("override leaked to next request: %q", got)
			}
			versions := map[string]bool{}
			for range 2 {
				rec := <-finished
				if rec.Code != http.StatusOK {
					t.Errorf("POST failed: %d %s", rec.Code, rec.Body.String())
				}
				versions[rec.Header().Get(ProjectVersionHeader)] = true
			}
			if !versions["main"] || !versions["feature/x"] {
				t.Errorf("wrong version headers: %v", versions)
			}
		})
	}
}

func TestGlobalASTPOSTRejectsInvalidScope(t *testing.T) {
	h := withProjectVersion(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { t.Error("invalid request reached handler") }), "")
	for _, body := range []string{`{`, `null`, `{"project_dir":3}`, `{"project_dir":"other"}`, `{"project_id":"other"}`, `{"context":"other"}`, `{"project_version":"other"}`, strings.Repeat("x", (1<<20)+1)} {
		rec := httptest.NewRecorder()
		h.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/query?project_dir=first&project_id=first&context=first&project_version=first", strings.NewReader(body)))
		if rec.Code != http.StatusBadRequest {
			t.Errorf("invalid body was accepted: status=%d", rec.Code)
		}
	}
}

func TestGlobalASTPOSTScopeMatchesJSONAliases(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	first, second := t.TempDir(), t.TempDir()
	a, _ := json.Marshal(first)
	b, _ := json.Marshal(second)
	for _, fields := range []string{
		`"project_dir":` + string(a) + `,"Project_Dir":` + string(b),
		`"Project_Dir":` + string(b) + `,"project_dir":` + string(a),
		`"project_dir":` + string(a) + `,"project_dir":` + string(b),
		`"Project_Dir":` + string(b) + `,"project_dir":null`,
	} {
		body := `{` + fields + `,"context":"first","Context":"second","project_version":"first","Project_Version":"main"}`
		called := false
		h := withProjectVersion(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			called = true
			var decoded struct {
				ProjectDir string `json:"project_dir"`
				Context    string `json:"context"`
			}
			if err := json.NewDecoder(r.Body).Decode(&decoded); err != nil {
				t.Fatal(err)
			}
			if r.URL.Query().Get("project_dir") != decoded.ProjectDir || r.URL.Query().Get("context") != decoded.Context {
				t.Error("middleware and handler selected different scope")
			}
			if projectversion.Current(decoded.ProjectDir).Version != "main" || w.Header().Get(ProjectVersionHeader) != "main" {
				t.Error("lease/header used a different project/version")
			}
		}), brand.GlobalDir())
		rec := httptest.NewRecorder()
		h.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/query", strings.NewReader(body)))
		if rec.Code != http.StatusOK || !called {
			t.Errorf("valid alias request rejected: %d %s", rec.Code, rec.Body.String())
		}
		called = false
		rec = httptest.NewRecorder()
		h.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/query?context=first", strings.NewReader(body)))
		if rec.Code != http.StatusBadRequest || called {
			t.Error("alias conflict bypassed validation")
		}
	}
}

// A Hub project has no checkout here: a request naming it by ID and a version is scoped
// to that version by ID, and the scope ends with the request.
func TestHubProjectRequestsAreScopedToTheSelectedVersion(t *testing.T) {
	const id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	var seen string
	h := withProjectVersion(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = projectversion.RemoteVersion(id)
	}), "")
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/tasks?project_id="+id+"&project_version="+url.QueryEscape("feature/r"), nil))
	if rec.Code != http.StatusOK || seen != "feature/r" || rec.Header().Get(ProjectVersionHeader) != "feature/r" {
		t.Fatalf("status %d, scoped version %q, header %q", rec.Code, seen, rec.Header().Get(ProjectVersionHeader))
	}
	if got := projectversion.RemoteVersion(id); got != "" {
		t.Fatalf("the scope outlived the request: %q", got)
	}
	h.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/api/tasks?project_id="+id, nil))
	if seen != "" {
		t.Fatalf("a request without a version selected %q, want the default version", seen)
	}
	rec = httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/tasks?project_id=not-a-ulid&project_version=x", nil))
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("an invalid project_id was accepted: %d", rec.Code)
	}
}

func TestWritesToAVersionWithoutSharedDataAreConflicts(t *testing.T) {
	notShared := "creating task: " + lancestore.ErrVersionNotShared.Error()
	for _, c := range []struct {
		status  int
		message string
		want    int
	}{
		{http.StatusInternalServerError, notShared, http.StatusConflict},
		{http.StatusInternalServerError, "disk full", http.StatusInternalServerError},
		{http.StatusBadRequest, notShared, http.StatusBadRequest},
	} {
		if got := versionAwareStatus(c.status, c.message); got != c.want {
			t.Errorf("versionAwareStatus(%d, %q) = %d, want %d", c.status, c.message, got, c.want)
		}
	}
	rec := httptest.NewRecorder()
	writeTaskError(rec, http.StatusInternalServerError, notShared)
	if rec.Code != http.StatusConflict {
		t.Fatalf("writeTaskError status = %d", rec.Code)
	}
}
