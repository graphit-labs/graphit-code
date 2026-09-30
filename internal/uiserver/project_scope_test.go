package uiserver

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const remoteProjectID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

func TestResolveProjectScopeKeepsRemoteIDOutOfFilesystemAddress(t *testing.T) {
	req := httptest.NewRequest(http.MethodGet, "/api/tasks?project_id="+remoteProjectID, nil)
	scope, err := resolveProjectScope(req, "/default/project")
	if err != nil {
		t.Fatal(err)
	}
	if !scope.Remote() || scope.ProjectID != remoteProjectID || scope.ProjectDir != "" {
		t.Fatalf("scope = %#v", scope)
	}
	if scope.Key() != "hub:"+remoteProjectID {
		t.Fatalf("key = %q", scope.Key())
	}
}

func TestResolveProjectScopeRejectsAmbiguousOrInvalidRemoteAddress(t *testing.T) {
	for _, rawURL := range []string{
		"/api/tasks?project_dir=%2Fproject&project_id=" + remoteProjectID,
		"/api/tasks?project_id=not-a-project-id",
	} {
		req := httptest.NewRequest(http.MethodGet, rawURL, nil)
		if _, err := resolveProjectScope(req, ""); err == nil {
			t.Fatalf("resolveProjectScope(%q) succeeded", rawURL)
		}
	}
}

func TestResolveProjectScopeRejectsGlobalDirectory(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	for _, rawURL := range []string{"/api/tasks", "/api/tasks?project_dir=" + globalDir} {
		req := httptest.NewRequest(http.MethodGet, rawURL, nil)
		if _, err := resolveProjectScope(req, globalDir); err == nil {
			t.Fatalf("global directory accepted for %s", rawURL)
		}
	}
	if _, err := resolveProjectScope(httptest.NewRequest(http.MethodGet, "/api/tasks", nil), ""); err == nil {
		t.Fatal("missing project accepted")
	}
}

func TestProjectCatalogHandlersRejectMissingAndGlobalScope(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	mux := http.NewServeMux()
	NewTaskHandler("").RegisterAPIRoutes(mux)
	NewSessionHandler("").RegisterAPIRoutes(mux)
	NewMemoryHandler("").RegisterAPIRoutes(mux)
	for _, path := range []string{
		"/api/tasks", "/api/tasks/sessions", "/api/memories?scope=project",
		"/api/tasks?project_dir=" + globalDir,
		"/api/tasks/sessions?project_dir=" + globalDir,
		"/api/memories?scope=project&project_dir=" + globalDir,
	} {
		response := httptest.NewRecorder()
		mux.ServeHTTP(response, httptest.NewRequest(http.MethodGet, path, nil))
		if response.Code != http.StatusBadRequest {
			t.Errorf("GET %s: status = %d, body = %s", path, response.Code, response.Body.String())
		}
	}
	if _, err := os.Stat(filepath.Join(globalDir, brand.LockFileName())); !os.IsNotExist(err) {
		t.Fatalf("catalog requests created project identity: %v", err)
	}
}
