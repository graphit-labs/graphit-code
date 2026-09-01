package store

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

// A Hub artifact can be installed with no project at all, and this is the record of
// those installs.
//
// Every compiled store in this framework is already global and keyed by id plus
// version — ASTHubDir, KnowledgeHubDir, and the Hub's own clone cache. Nothing about
// the DATA was ever per-project. What is per-project is MEMBERSHIP: the lockfile entry
// that says this project may reach that store, read back by LookupContext. So an
// install that belongs to no project needs no new store layout; it needs somewhere
// other than a project lockfile to record the same membership.
//
// That somewhere already existed. The global lock at <global>/global.lock.json has
// recorded every install per artifact version since it was introduced, carrying the id,
// the version, the type and the projects that asked for it. An install with no project
// is one more entry there, claimed by the reserved GlobalOwnerKey instead of by a
// project id.
//
// This file is the READ side, and it is deliberately only that. internal/hub writes the
// global lock; internal/store cannot import internal/hub — hub imports ast, knowledge
// and memory, which all import this package — so the few fields needed here are read
// through an anonymous struct. That is the same asymmetry ProjectID already has with the
// project lockfile it does not own.

// GlobalOwnerKey is the reserved owner of an install that belongs to no project.
//
// It matches the key the Hub already uses for artifacts published outside any project,
// so the two notions of "global" read the same in the files a user may open.
const GlobalOwnerKey = "_global"

// globalLockFileName is the global lock's file name. It is duplicated from
// internal/hub rather than imported, for the dependency reason above.
const globalLockFileName = "global.lock.json"

// globalArtifactRecord is the slice of a global-lock artifact entry that context
// resolution needs. The entry carries more — name, description, hash, cache path — and
// none of it decides where a store lives.
type globalArtifactRecord struct {
	ID        string                     `json:"id"`
	Version   string                     `json:"version"`
	Type      string                     `json:"type"`
	ProjectID string                     `json:"projectId,omitempty"`
	Projects  map[string]json.RawMessage `json:"projects"`
}

// ownedGlobally reports whether the reserved owner claimed this artifact.
//
// An entry whose only owners are real projects is NOT globally installed: it is in the
// global lock because that is where every install is recorded, and answering a
// project-less query from it would hand out a store the caller never asked for.
func (r globalArtifactRecord) ownedGlobally() bool {
	_, ok := r.Projects[GlobalOwnerKey]
	return ok
}

func globalLockPath() string {
	root := Root()
	if root == "" {
		return ""
	}
	return filepath.Join(root, globalLockFileName)
}

func loadGlobalArtifacts() []globalArtifactRecord {
	path := globalLockPath()
	if path == "" {
		return nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	var lock struct {
		Artifacts map[string]globalArtifactRecord `json:"artifacts"`
	}
	if json.Unmarshal(data, &lock) != nil {
		return nil
	}
	out := make([]globalArtifactRecord, 0, len(lock.Artifacts))
	for _, rec := range lock.Artifacts {
		out = append(out, rec)
	}
	// The map iteration order is random, and LookupGlobalContext resolves an
	// unversioned name by taking the highest version. Sorting here makes both that
	// choice and ListGlobalContexts deterministic between runs.
	sort.Slice(out, func(i, j int) bool {
		if out[i].ID != out[j].ID {
			return out[i].ID < out[j].ID
		}
		return out[i].Version < out[j].Version
	})
	return out
}

func globalRecordFrom(rec globalArtifactRecord) ContextRecord {
	return ContextRecord{
		Name:       ContextNameFor(rec.ID, rec.ProjectID),
		ArtifactID: rec.ID,
		Version:    rec.Version,
		Origin:     projectlock.OriginHub,
		ProjectID:  rec.ProjectID,
	}
}

// ListGlobalContexts returns the contexts of one kind installed with no project, keyed
// by the name each is known by.
//
// When two versions of the same artifact are installed globally, the highest version
// wins the name — the same artifact cannot mean two stores under one name. Both remain
// reachable by their qualified id@version.
func ListGlobalContexts(kind string) map[string]ContextRecord {
	out := map[string]ContextRecord{}
	for _, rec := range loadGlobalArtifacts() {
		if rec.Type != kind || !rec.ownedGlobally() {
			continue
		}
		cr := globalRecordFrom(rec)
		if existing, ok := out[cr.Name]; ok && existing.Version > cr.Version {
			continue
		}
		out[cr.Name] = cr
	}
	return out
}

// GlobalContextNames returns the names of one kind's globally installed contexts,
// sorted, so that output built from them is stable between runs.
func GlobalContextNames(kind string) []string {
	ctxs := ListGlobalContexts(kind)
	names := make([]string, 0, len(ctxs))
	for name := range ctxs {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// SplitQualified separates a qualified artifact reference into its id and version.
//
// "demo@1.2.0" is the form the Hub already accepts on install, and it is what a
// project-less caller has instead of a project: there is no lockfile to read the
// version from, so the reference has to carry it. A bare name is legitimate and means
// "whichever version is installed".
func SplitQualified(ref string) (id, version string) {
	if i := strings.LastIndex(ref, "@"); i > 0 {
		return ref[:i], ref[i+1:]
	}
	return ref, ""
}

// LookupGlobalContext resolves one context name against the global installs.
//
// The name is accepted in every form a caller may have been shown it in: the context
// name (the publishing project's id, or the artifact id when there is none), the
// artifact id, either of those sanitised, and any of them qualified with @version.
func LookupGlobalContext(kind, name string) (ContextRecord, bool) {
	if name == "" {
		return ContextRecord{}, false
	}
	wantName, wantVersion := SplitQualified(name)
	wantSanitized := SanitizeName(wantName)

	var best ContextRecord
	found := false
	for _, rec := range loadGlobalArtifacts() {
		if rec.Type != kind || !rec.ownedGlobally() {
			continue
		}
		if wantVersion != "" && rec.Version != wantVersion {
			continue
		}
		cr := globalRecordFrom(rec)
		if cr.Name != wantName && cr.ArtifactID != wantName &&
			SanitizeName(cr.Name) != wantSanitized && SanitizeName(cr.ArtifactID) != wantSanitized {
			continue
		}
		// Unqualified: the highest installed version answers, so that a caller who
		// does not care gets the newest rather than whichever the map yielded first.
		if found && best.Version > cr.Version {
			continue
		}
		best, found = cr, true
	}
	return best, found
}
