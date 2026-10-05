package memory

import (
	"net/url"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// projectBranchForURI is the native Lance branch a memory table at uri is read and
// written on.
//
// Only project memory is versioned: a project's memory belongs to the version checked
// out — its Git branch, or main outside Git — and a version without memory of its own
// starts from the default version's. User memory and imported contexts have no
// version and always use the main lineage, as does project memory reached from a
// directory that is not that project, since there is no checkout to take a version
// from. projectDir names the checkout; empty means the project the process sits in.
func projectBranchForURI(uri, projectDir string) string {
	id := projectIDOfMemoryURI(uri)
	if id == "" {
		return ""
	}
	if projectDir == "" {
		projectDir = projectRootDir()
	}
	if projectDir == "" || store.ProjectID(projectDir) != id {
		return ""
	}
	return projectversion.Current(projectDir).LanceBranch()
}

// projectIDOfMemoryURI is the project a project-memory table URI belongs to, or "" for
// any other scope: locally `<root>/memory/memory-project-<id>`, in S3
// `s3://<bucket>/<prefix>/v2/projects/<id>/memory`.
func projectIDOfMemoryURI(uri string) string {
	if strings.HasPrefix(uri, "s3://") {
		parsed, err := url.Parse(uri)
		if err != nil {
			return ""
		}
		parts := strings.Split(strings.Trim(parsed.Path, "/"), "/")
		n := len(parts)
		if n >= 3 && parts[n-1] == "memory" && parts[n-3] == "projects" {
			return parts[n-2]
		}
		return ""
	}
	id, ok := strings.CutPrefix(filepath.Base(filepath.Clean(uri)), "memory-project-")
	if !ok {
		return ""
	}
	return id
}
