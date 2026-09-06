package store

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

// GlobalOwnerKey is the reserved local owner of an install that belongs to no checkout.
const GlobalOwnerKey = "_global"

const globalLockFileName = "global.lock.json"

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
		Name:       rec.ProjectID,
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
		if rec.Type != kind || !rec.ownedGlobally() || rec.ProjectID == "" {
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
// The name is accepted as the publishing project ID or artifact ID, optionally versioned.
func LookupGlobalContext(kind, name string) (ContextRecord, bool) {
	if name == "" {
		return ContextRecord{}, false
	}
	wantName, wantVersion := SplitQualified(name)
	wantSanitized := SanitizeName(wantName)

	var best ContextRecord
	found := false
	for _, rec := range loadGlobalArtifacts() {
		if rec.Type != kind || !rec.ownedGlobally() || rec.ProjectID == "" {
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
		if found && best.Version > cr.Version {
			continue
		}
		best, found = cr, true
	}
	return best, found
}
