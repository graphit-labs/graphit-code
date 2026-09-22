package uiserver

import (
	"net/http"
	"net/http/httptest"
	"testing"
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
