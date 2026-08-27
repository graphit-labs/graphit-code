package store

import (
	"fmt"
	"path/filepath"
	"sort"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

// Context membership lives in the project's lockfile, and nowhere else.
//
// It used to be split in two. A Hub AST artifact was resolved from the lockfile, while
// a locally imported context, a link, and a Hub KNOWLEDGE artifact were resolved from a
// separate `contexts.json` beside it. The split followed no principle — it followed the
// store path: an AST Hub store is keyed by version, so resolving one required reading
// the version, and the version was in the lockfile. A knowledge store was not keyed by
// version, so its resolution never needed the lockfile and never learned to read it.
//
// That had a consequence nobody chose: a knowledge context was not versioned at all.
// Two projects pinned to different versions of the same artifact shared one directory
// and the last install won, while the lockfile recorded a version nothing enforced.
//
// So there is one record now. `hub install` already wrote a lockfile entry with a
// version for every artifact type, which means unification mostly meant deleting the
// second registry and teaching resolution to read the first.

// ContextRecord is one project's claim on one imported context.
//
// It is a view over a lockfile artifact entry, not a separate stored shape.
type ContextRecord struct {
	// Name is what the context is known by: its directory name for a local import,
	// and the publishing project's ID for a Hub artifact.
	Name string
	// ArtifactID is the lockfile key. It differs from Name for a Hub artifact
	// published by a project, where the name is the project ID.
	ArtifactID string
	// Version is a Hub version, or projectlock.VersionLocal.
	Version string
	Origin  string
	// ProjectID is the publishing project of a Hub artifact, and part of its store
	// key.
	ProjectID string
	// SourcePath is ABSOLUTE, resolved from the project-relative value the lockfile
	// stores. Empty for a Hub artifact, which has no local source.
	SourcePath string
}

// IsHub reports whether the context came from the Hub, and therefore lives in a
// version-keyed store.
func (r ContextRecord) IsHub() bool {
	return r.Origin != projectlock.OriginLink &&
		r.Origin != projectlock.OriginLocal &&
		r.Version != projectlock.VersionLocal &&
		r.Version != ""
}

// IsLink reports whether the context points at a store this project does not own —
// a sibling project's, reached in place.
func (r ContextRecord) IsLink() bool { return r.Origin == projectlock.OriginLink }

// ContextNameFor is the name a lockfile entry is known by.
//
// A Hub artifact published by a project is named after that project, because that is
// what keys its store; one published outside any project falls back to the artifact ID,
// or its store would be built somewhere no lookup could name.
func ContextNameFor(artifactID, projectID string) string {
	if projectID != "" {
		return projectID
	}
	return artifactID
}

func lockPathFor(projectDir string) string {
	return filepath.Join(projectDir, brand.LockFileName())
}

func recordFrom(projectDir, artifactID string, meta *projectlock.ArtifactMeta) ContextRecord {
	return ContextRecord{
		Name:       ContextNameFor(artifactID, meta.ProjectID),
		ArtifactID: artifactID,
		Version:    meta.Version,
		Origin:     meta.Origin,
		ProjectID:  meta.ProjectID,
		SourcePath: projectlock.SourceDir(projectDir, meta.SourcePath),
	}
}

// ListContexts returns the contexts of one kind that projectDir has claimed, keyed by
// the name each is known by.
//
// An empty map is the normal answer for a project that has claimed nothing, and is not
// distinguished from a missing lockfile: both mean the same thing.
func ListContexts(projectDir, kind string) map[string]ContextRecord {
	out := map[string]ContextRecord{}
	if projectDir == "" {
		return out
	}
	lf, err := projectlock.Load(lockPathFor(projectDir))
	if err != nil || lf == nil {
		return out
	}
	for artifactID, meta := range lf.Artifacts[projectlock.ArtifactType(kind)] {
		if meta == nil {
			continue
		}
		rec := recordFrom(projectDir, artifactID, meta)
		out[rec.Name] = rec
	}
	return out
}

// ContextNames returns the names of one kind's contexts, sorted, so that output built
// from them is stable between runs.
func ContextNames(projectDir, kind string) []string {
	ctxs := ListContexts(projectDir, kind)
	names := make([]string, 0, len(ctxs))
	for name := range ctxs {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// LookupContext resolves one context name for a project, accepting the name as given,
// its sanitised form, or the Hub artifact ID it was installed under.
func LookupContext(projectDir, kind, name string) (ContextRecord, bool) {
	if name == "" {
		return ContextRecord{}, false
	}
	ctxs := ListContexts(projectDir, kind)
	if rec, ok := ctxs[name]; ok {
		return rec, true
	}
	if rec, ok := ctxs[SanitizeName(name)]; ok {
		return rec, true
	}
	// Callers echo back whichever name they were shown, and for a Hub artifact that
	// is sometimes the artifact ID rather than the publishing project.
	for _, rec := range ctxs {
		if rec.ArtifactID == name || SanitizeName(rec.ArtifactID) == SanitizeName(name) {
			return rec, true
		}
	}
	return ContextRecord{}, false
}

// HasContext reports whether a project has claimed a context of this kind.
func HasContext(projectDir, kind, name string) bool {
	_, ok := LookupContext(projectDir, kind, name)
	return ok
}

// AddContext records a local context — an import or a link — against a project.
//
// It is NOT how a Hub artifact is recorded: `hub install` writes its own lockfile entry
// with the version, hash and members it resolved, and this would only overwrite that
// with less. Kept for the two origins the Hub does not describe.
//
// SourcePath is stored relative to the project, so the lockfile can be committed and
// still resolve on a teammate's clone.
func AddContext(projectDir, kind string, rec ContextRecord) error {
	if projectDir == "" {
		return fmt.Errorf("context registry: no project directory")
	}
	if rec.Name == "" {
		return fmt.Errorf("context registry: name is required")
	}

	path := lockPathFor(projectDir)
	lf, err := projectlock.Load(path)
	if err != nil {
		return err
	}
	if lf == nil {
		lf = &projectlock.Lockfile{}
	}
	if lf.Artifacts == nil {
		lf.Artifacts = map[projectlock.ArtifactType]map[string]*projectlock.ArtifactMeta{}
	}
	at := projectlock.ArtifactType(kind)
	if lf.Artifacts[at] == nil {
		lf.Artifacts[at] = map[string]*projectlock.ArtifactMeta{}
	}

	origin := rec.Origin
	if origin == "" {
		origin = projectlock.OriginLocal
	}
	version := rec.Version
	if version == "" {
		version = projectlock.VersionLocal
	}

	key := SanitizeName(rec.Name)
	meta := lf.Artifacts[at][key]
	if meta == nil {
		meta = &projectlock.ArtifactMeta{}
		lf.Artifacts[at][key] = meta
	}
	meta.Version = version
	meta.Origin = origin
	meta.SourcePath = ""
	if rec.SourcePath != "" {
		meta.SourcePath = projectlock.RelSourcePath(projectDir, rec.SourcePath)
	}

	return projectlock.Save(path, lf)
}

// RemoveContext drops a project's claim on a context. It does NOT delete the store:
// another project may have claimed the same one, and the store is shared.
func RemoveContext(projectDir, kind, name string) error {
	if projectDir == "" {
		return fmt.Errorf("context registry: no project directory")
	}
	path := lockPathFor(projectDir)
	lf, err := projectlock.Load(path)
	if err != nil {
		return err
	}
	if lf == nil {
		return fmt.Errorf("context %q not found", name)
	}
	at := projectlock.ArtifactType(kind)
	entries := lf.Artifacts[at]
	if entries == nil {
		return fmt.Errorf("context %q not found", name)
	}

	key := ""
	for artifactID, meta := range entries {
		if meta == nil {
			continue
		}
		if artifactID == name || SanitizeName(artifactID) == SanitizeName(name) ||
			ContextNameFor(artifactID, meta.ProjectID) == name {
			key = artifactID
			break
		}
	}
	if key == "" {
		return fmt.Errorf("context %q not found", name)
	}
	delete(entries, key)
	return projectlock.Save(path, lf)
}
