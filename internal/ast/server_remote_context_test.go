//go:build lancedb

package ast

import (
	"context"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"
)

func TestRemoteContextResolverRequiresAndPreservesExactIdentity(t *testing.T) {
	srv, err := NewServerOnPort(&emptyGraphDB{}, t.TempDir(), 0)
	if err != nil {
		t.Fatalf("NewServerOnPort: %v", err)
	}
	wantStore := filepath.Join(t.TempDir(), "artifact-store")
	var gotProject, gotContext string
	srv.SetExternalContextResolver(func(_ context.Context, projectID, qualified string) (string, string, error) {
		gotProject, gotContext = projectID, qualified
		return wantStore, "2.1.0", nil
	})

	req := httptest.NewRequest(http.MethodGet, "/api/file?project_id=01HUB&context=artifact%402.1.0&path=main.go", nil)
	if got := srv.storePathForRequest(req); got != wantStore {
		t.Fatalf("store path = %q, want %q", got, wantStore)
	}
	if gotProject != "01HUB" || gotContext != "artifact@2.1.0" {
		t.Fatalf("resolver identity = %q / %q", gotProject, gotContext)
	}

	invalid := httptest.NewRequest(http.MethodGet, "/api/schema?project_id=01HUB&context=artifact", nil)
	if _, err := srv.dbForContext(invalid).Query(context.Background(), "RETURN 1", nil); err == nil {
		t.Fatal("unqualified Hub context unexpectedly resolved")
	}
}
