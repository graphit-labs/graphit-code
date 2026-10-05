//go:build lancedb

package uiserver

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"github.com/graphit-labs/graphit-code/internal/store"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func versionsGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(gitenv.WithoutInvocationScope(os.Environ()), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@example.com",
		"GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@example.com",
		"GIT_CONFIG_GLOBAL="+os.DevNull, "GIT_CONFIG_SYSTEM="+os.DevNull)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

func getVersions(t *testing.T, query string) projectVersionsResponse {
	t.Helper()
	rec := httptest.NewRecorder()
	handleProjectVersions("").ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/project/versions?"+query, nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("%s: status %d %s", query, rec.Code, rec.Body.String())
	}
	var resp projectVersionsResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	return resp
}

func versionNamed(resp projectVersionsResponse, name string) *projectVersion {
	for i := range resp.Versions {
		if resp.Versions[i].Name == name {
			return &resp.Versions[i]
		}
	}
	return nil
}

// The versions of a project are derived from its stores and checkouts: the default
// and current version, a worktree's branch with the tasks it wrote, and a version whose
// branch was deleted from Git but still has a local index.
func TestProjectVersionsAreDerivedFromStoresAndCheckouts(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	t.Setenv(brand.EnvVar("GIT_BASE_BRANCH"), "")
	const id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	mainDir := t.TempDir()
	versionsGit(t, mainDir, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()), []byte(`{"project":{"id":"`+id+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	versionsGit(t, mainDir, "add", ".")
	versionsGit(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	versionsGit(t, mainDir, "worktree", "add", "-q", "-b", "feature/u", featureDir)

	ctx := context.Background()
	for dir, title := range map[string]string{mainDir: "on-main", featureDir: "on-feature"} {
		svc, err := graphtask.Open(dir)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := svc.Create(ctx, graphtask.CreateInput{Title: title, Description: title, AcceptanceCriteria: []string{"a"},
			Tests: []string{"t"}, IdempotencyKey: title, Actor: "tester"}); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.MkdirAll(store.VersionStoreDir(store.ASTProjectRoot(mainDir), "old/gone"), 0o755); err != nil {
		t.Fatal(err)
	}

	resp := getVersions(t, "project_dir="+url.QueryEscape(featureDir))
	if resp.Remote || !resp.Git || resp.Default != "main" || resp.Current != "feature/u" {
		t.Fatalf("summary = %+v", resp)
	}
	if len(resp.Versions) != 3 || resp.Versions[0].Name != "main" || resp.Versions[1].Name != "feature/u" {
		t.Fatalf("versions out of order: %+v", resp.Versions)
	}
	main, feature, gone := versionNamed(resp, "main"), versionNamed(resp, "feature/u"), versionNamed(resp, "old/gone")
	if !main.Default || !main.InGit || len(main.Checkouts) != 1 {
		t.Errorf("main = %+v", *main)
	}
	if !feature.Current || !feature.InGit || len(feature.Checkouts) != 1 || !has(feature.Sources, "task") || !has(feature.Sources, "checkout") {
		t.Errorf("feature/u = %+v", *feature)
	}
	if gone.InGit || !has(gone.Sources, "ast") || len(gone.Checkouts) != 0 {
		t.Errorf("old/gone = %+v", *gone)
	}

	// The same project reached by ID lists its default version and its Lance branches.
	remote := getVersions(t, "project_id="+id)
	if !remote.Remote || len(remote.Versions) != 2 || remote.Versions[0].Name != "" || !remote.Versions[0].Default {
		t.Fatalf("remote versions = %+v", remote)
	}
	if v := versionNamed(remote, "feature/u"); v == nil || !has(v.Sources, "task") {
		t.Fatalf("remote feature/u = %+v", remote.Versions)
	}
}

// A UI request for a Hub project names it by ID and the version: Task reads and Memory
// writes reach that version's branch, and a write to a version without one is a conflict.
func TestHubProjectRequestsReachTheSelectedVersion(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	const id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	mainDir := t.TempDir()
	versionsGit(t, mainDir, "init", "-q", "-b", "main")
	if err := os.WriteFile(filepath.Join(mainDir, brand.LockFileName()), []byte(`{"project":{"id":"`+id+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	versionsGit(t, mainDir, "add", ".")
	versionsGit(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	versionsGit(t, mainDir, "worktree", "add", "-q", "-b", "feature/u", featureDir)
	svc, err := graphtask.Open(featureDir)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Create(context.Background(), graphtask.CreateInput{Title: "on-feature", Description: "d",
		AcceptanceCriteria: []string{"a"}, Tests: []string{"t"}, IdempotencyKey: "on-feature", Actor: "tester"}); err != nil {
		t.Fatal(err)
	}

	mux := http.NewServeMux()
	NewTaskHandler("").RegisterAPIRoutes(mux)
	NewMemoryHandler("").RegisterAPIRoutes(mux)
	h := withProjectVersion(mux, "")
	do := func(method, target, body string) *httptest.ResponseRecorder {
		t.Helper()
		var req *http.Request
		if body == "" {
			req = httptest.NewRequest(method, target, nil)
		} else {
			req = httptest.NewRequest(method, target, strings.NewReader(body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("X-Graphit-Request", "ui")
		}
		rec := httptest.NewRecorder()
		h.ServeHTTP(rec, req)
		return rec
	}
	byID := "project_id=" + id
	if rec := do(http.MethodGet, "/api/tasks?"+byID+"&project_version="+url.QueryEscape("feature/u"), ""); rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), "on-feature") {
		t.Fatalf("feature/u tasks by ID: %d %s", rec.Code, rec.Body.String())
	}
	if rec := do(http.MethodGet, "/api/tasks?"+byID, ""); rec.Code != http.StatusOK || strings.Contains(rec.Body.String(), "on-feature") {
		t.Fatalf("default-version tasks by ID: %d %s", rec.Code, rec.Body.String())
	}

	if rec := do(http.MethodPost, "/api/memories?project_dir="+url.QueryEscape(featureDir), `{"title":"from-checkout","body":"b"}`); rec.Code != http.StatusCreated {
		t.Fatalf("memory from the feature checkout: %d %s", rec.Code, rec.Body.String())
	}
	if rec := do(http.MethodPost, "/api/memories?"+byID+"&project_version="+url.QueryEscape("feature/u"), `{"title":"from-hub","body":"b"}`); rec.Code != http.StatusCreated {
		t.Fatalf("memory written by ID to feature/u: %d %s", rec.Code, rec.Body.String())
	}
	if rec := do(http.MethodGet, "/api/memories?project_dir="+url.QueryEscape(featureDir), ""); !strings.Contains(rec.Body.String(), "from-hub") {
		t.Fatalf("the write by ID missed feature/u: %s", rec.Body.String())
	}
	if rec := do(http.MethodGet, "/api/memories?project_dir="+url.QueryEscape(mainDir), ""); strings.Contains(rec.Body.String(), "from-hub") {
		t.Fatalf("the write by ID reached main: %s", rec.Body.String())
	}
	if rec := do(http.MethodPost, "/api/memories?"+byID+"&project_version="+url.QueryEscape("feature/none"), `{"title":"refused","body":"b"}`); rec.Code != http.StatusConflict {
		t.Fatalf("a write to a version without shared memory: %d %s, want 409", rec.Code, rec.Body.String())
	}
}

func has(list []string, value string) bool {
	for _, v := range list {
		if v == value {
			return true
		}
	}
	return false
}
