package uiserver

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

// ProjectVersionHeader reports, on every response for a project, which version of
// it the request was answered from.
const ProjectVersionHeader = "X-Project-Version"

// withProjectVersion scopes each request for a project to one project version.
//
// A project's stores are kept per version — the branch checked out in its
// directory, or main without Git. `project_version` selects another version for the
// request: it holds the directory exclusively while it runs, so every store resolver
// it reaches answers that version and concurrent requests for the same directory
// wait instead of reading the wrong store. The resolved version is returned in
// ProjectVersionHeader.
func withProjectVersion(next http.Handler, defaultProjectDir string) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := normalizeASTScope(r); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		query := r.URL.Query()
		version := strings.TrimSpace(query.Get("project_version"))
		dir := strings.TrimSpace(query.Get("project_dir"))
		if dir == "" && query.Get("project_id") == "" {
			dir = strings.TrimSpace(defaultProjectDir)
		}
		if dir == "" || brand.IsGlobalDir(dir) {
			if version != "" {
				http.Error(w, "project_version selects a version of the project in project_dir, and no project_dir was given", http.StatusBadRequest)
				return
			}
			next.ServeHTTP(w, r)
			return
		}
		if abs, err := filepath.Abs(dir); err == nil {
			dir = abs
		}
		release, err := projectversion.Acquire(dir, version)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		defer release()
		w.Header().Set(ProjectVersionHeader, projectversion.Current(dir).Version)
		next.ServeHTTP(w, r)
	})
}

// AST POST clients send their selection in JSON. Resolve it before acquiring the
// version lease, preserving the body for the handler. Conflicting selections
// would otherwise lease one project and query another.
func normalizeASTScope(r *http.Request) error {
	if r.Method != http.MethodPost || (r.URL.Path != "/api/query" && r.URL.Path != "/api/generate-cypher") {
		return nil
	}
	const limit = 1 << 20
	data, err := io.ReadAll(io.LimitReader(r.Body, limit+1))
	if err != nil || len(data) > limit {
		return fmt.Errorf("invalid AST request body")
	}
	r.Body = io.NopCloser(bytes.NewReader(data))
	// Use the same tagged struct decoding as the handlers: JSON field names
	// match without case sensitivity and later aliases can replace earlier ones.
	var body struct {
		ProjectDir     string `json:"project_dir"`
		ProjectID      string `json:"project_id"`
		Context        string `json:"context"`
		ProjectVersion string `json:"project_version"`
	}
	if err := json.Unmarshal(data, &body); err != nil || bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return fmt.Errorf("invalid AST request body")
	}
	query := r.URL.Query()
	for _, field := range []struct{ key, value string }{
		{"project_dir", body.ProjectDir}, {"project_id", body.ProjectID},
		{"context", body.Context}, {"project_version", body.ProjectVersion},
	} {
		key, value := field.key, field.value
		if value == "" {
			continue
		}
		if current := query.Get(key); current != "" && current != value {
			return fmt.Errorf("conflicting AST %s", key)
		}
		query.Set(key, value)
	}
	r.URL.RawQuery = query.Encode()
	return nil
}
