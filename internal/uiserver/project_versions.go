package uiserver

import (
	"context"
	"net/http"
	"slices"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/store"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

// projectVersion is one version of a project the UI can select.
type projectVersion struct {
	// Name is the version — a Git branch name. For a Hub project the default version
	// has no known name and is listed with an empty one: select it by omitting
	// project_version.
	Name      string   `json:"name"`
	Default   bool     `json:"default"`
	Current   bool     `json:"current"`
	Checkouts []string `json:"checkouts,omitempty"`
	// InGit reports, for a local project under Git, whether the branch still exists.
	InGit bool `json:"in_git"`
	// Sources are where the version has data or is checked out: ast, knowledge, task,
	// memory, dream, checkout.
	Sources []string `json:"sources"`
}

type projectVersionsResponse struct {
	Remote   bool             `json:"remote"`
	Git      bool             `json:"git"`
	Default  string           `json:"default"`
	Current  string           `json:"current"`
	Versions []projectVersion `json:"versions"`
	// Warnings name the sources that could not be listed; the list is still useful.
	Warnings []string `json:"warnings,omitempty"`
}

// handleProjectVersions lists the versions of a project, derived on the fly from where
// each kind of data lives: the native Lance branches of its Task, Memory and Dream
// tables, its local AST and Knowledge stores, and the branch each checkout has out.
// Nothing records versions separately.
func handleProjectVersions(defaultProjectDir string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		scope, err := resolveProjectScope(r, defaultProjectDir)
		if err != nil {
			w.WriteHeader(http.StatusBadRequest)
			writeJSON(w, map[string]string{"error": err.Error()})
			return
		}
		if scope.Remote() {
			writeJSON(w, remoteProjectVersions(r.Context(), scope.ProjectID))
			return
		}
		writeJSON(w, localProjectVersions(r.Context(), scope.ProjectDir))
	}
}

type versionSet struct {
	byName map[string]*projectVersion
}

func (s *versionSet) get(name string) *projectVersion {
	if s.byName == nil {
		s.byName = map[string]*projectVersion{}
	}
	v := s.byName[name]
	if v == nil {
		v = &projectVersion{Name: name}
		s.byName[name] = v
	}
	return v
}

func (s *versionSet) addSource(name, source string) {
	v := s.get(name)
	for _, existing := range v.Sources {
		if existing == source {
			return
		}
	}
	v.Sources = append(v.Sources, source)
}

func (s *versionSet) addBranches(source string, branches []string, err error, warnings *[]string) {
	if err != nil {
		*warnings = append(*warnings, source+": "+err.Error())
		return
	}
	for _, branch := range branches {
		s.addSource(projectversion.VersionFromLanceBranch(branch), source)
	}
}

// sorted orders the default version first, then the current one, then by name.
func (s *versionSet) sorted() []projectVersion {
	out := make([]projectVersion, 0, len(s.byName))
	for _, v := range s.byName {
		sort.Strings(v.Sources)
		if v.Sources == nil {
			v.Sources = []string{}
		}
		out = append(out, *v)
	}
	rank := func(v projectVersion) int {
		switch {
		case v.Default:
			return 0
		case v.Current:
			return 1
		default:
			return 2
		}
	}
	sort.Slice(out, func(i, j int) bool {
		if ri, rj := rank(out[i]), rank(out[j]); ri != rj {
			return ri < rj
		}
		return out[i].Name < out[j].Name
	})
	return out
}

func localProjectVersions(ctx context.Context, projectDir string) projectVersionsResponse {
	checkout := projectversion.Checkout(projectDir)
	resp := projectVersionsResponse{Git: checkout.Git, Default: checkout.Default, Current: checkout.Version}
	var set versionSet
	set.get(checkout.Default).Default = true
	set.get(checkout.Version).Current = true

	for version, modules := range store.LocalVersions(projectDir) {
		for _, module := range modules {
			set.addSource(version, module)
		}
	}
	if svc, err := graphtask.Open(projectDir); err == nil {
		branches, err := svc.VersionBranches(ctx)
		set.addBranches("task", branches, err, &resp.Warnings)
	}
	if id := store.ProjectID(projectDir); id != "" {
		branches, err := memory.ProjectVersionBranches(ctx, id)
		set.addBranches("memory", branches, err, &resp.Warnings)
		branches, err = dream.VersionBranches(ctx, projectDir)
		set.addBranches("dream", branches, err, &resp.Warnings)
	}
	for _, wt := range projectversion.Worktrees(projectDir) {
		if wt.Detached {
			continue
		}
		v := set.get(wt.Version)
		v.Checkouts = append(v.Checkouts, wt.Dir)
		set.addSource(wt.Version, "checkout")
	}
	for name, v := range set.byName {
		v.InGit = checkout.Git && projectversion.BranchExists(projectDir, name)
	}
	resp.Versions = set.sorted()
	return resp
}

func remoteProjectVersions(ctx context.Context, projectID string) projectVersionsResponse {
	resp := projectVersionsResponse{Remote: true}
	var set versionSet
	set.get("").Default = true
	if svc, err := graphtask.OpenProject(ctx, projectID); err != nil {
		resp.Warnings = append(resp.Warnings, "task: "+err.Error())
	} else {
		branches, err := svc.VersionBranches(ctx)
		set.addBranches("task", branches, err, &resp.Warnings)
	}
	branches, err := memory.ProjectVersionBranches(ctx, projectID)
	set.addBranches("memory", branches, err, &resp.Warnings)
	resp.Versions = set.sorted()
	return resp
}

// versionAwareStatus answers a write to a project version that has no shared data in a
// Hub project's table yet with 409 instead of a server error: the request is valid, the
// version simply has to be written from a checkout first.
func versionAwareStatus(status int, message string) int {
	if status >= 500 && strings.Contains(message, lancestore.ErrVersionNotShared.Error()) {
		return http.StatusConflict
	}
	return status
}

// remoteVersionConfirmed reports whether version of a Hub project already has a native
// Lance branch in its Task or Memory store, which proves it is a real version rather than
// the default one under its own name. Its other stores may then create the branch on a
// write. A confirmation is cached briefly; a refusal is not, so a version becomes
// writable as soon as a checkout shares it.
func remoteVersionConfirmed(ctx context.Context, projectID, version string) bool {
	key := projectID + "\x00" + version
	if until, ok := confirmedRemoteVersions.Load(key); ok && time.Now().Before(until.(time.Time)) {
		return true
	}
	branch := projectversion.LanceBranchName(version, "")
	found := false
	if svc, err := graphtask.OpenProject(ctx, projectID); err == nil {
		if branches, err := svc.VersionBranches(ctx); err == nil && slices.Contains(branches, branch) {
			found = true
		}
	}
	if !found {
		if branches, err := memory.ProjectVersionBranches(ctx, projectID); err == nil && slices.Contains(branches, branch) {
			found = true
		}
	}
	if found {
		confirmedRemoteVersions.Store(key, time.Now().Add(confirmedRemoteVersionTTL))
	}
	return found
}

const confirmedRemoteVersionTTL = 30 * time.Second

var confirmedRemoteVersions sync.Map // project ID + version -> expiry
