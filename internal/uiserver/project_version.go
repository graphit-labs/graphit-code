package uiserver

import (
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
