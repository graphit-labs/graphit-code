package ast

import (
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// HubContextsRoot is the parent of every version-scoped Hub AST store.
func HubContextsRoot() string { return store.ASTHubRoot() }

// HubContextDir is the shared directory holding one version of a Hub AST context.
func HubContextDir(projectID, version string) string {
	return store.ASTHubDir(projectID, version)
}

// HubContextID is the name a Hub AST context is known by: the publishing project's
// ID, which is also its directory name. An artifact published outside any project
// has no project ID, so it falls back to the artifact ID — otherwise its store
// would be built somewhere no lookup could name.
func HubContextID(projectID, artifactID string) string {
	if projectID != "" {
		return projectID
	}
	return artifactID
}

// IsUnderHubContextsRoot reports whether path lies inside the shared store root.
//
// The containment test goes through filepath.Rel rather than a string prefix
// because a prefix comparison is wrong on two of the three platforms this ships
// to: Windows mixes `\` and `/` and compares paths case-insensitively, and macOS
// is case-insensitive too, so `HasPrefix` both misses real matches and would
// accept a sibling directory whose name merely starts with the root's. Rel
// normalises separators and reports escape as a leading "..".
//
// Case folding is deliberately NOT applied: every caller builds both sides from
// HubContextsRoot, so the casing already agrees, and folding would make the check
// accept paths on Linux that the filesystem treats as distinct.
func IsUnderHubContextsRoot(path string) bool {
	root := HubContextsRoot()
	if root == "" || path == "" {
		return false
	}
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return false
	}
	if rel == "." || rel == ".." {
		return false
	}
	return !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}
