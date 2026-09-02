package hub

import (
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// Mounting a published artifact: reading it where it lives instead of downloading it.
//
// THE URI IS DERIVED, NEVER STORED, and that is the decision this file rests on. A context record
// already carries everything the location is made of — the artifact id, the version and the
// publishing project — so the URI is computed from those plus the configured bucket. Writing it
// into the lockfile would have been the obvious move and would be wrong twice over: it changes a
// format every project on disk already has, and it freezes an endpoint, so pointing the framework
// at a different bucket would leave every installed context resolving to the old one.
//
// WHAT IS MOUNTABLE TODAY: the wiki half. A knowledge artifact is a wiki and has no graph, so it
// mounts completely. An AST artifact has both, and its GRAPH cannot be mounted yet — the icebug
// format holds one CSR per relationship table while this graph has ~97 distinct (from,to) pairs,
// so a mounted graph would silently answer with the same edges counted once per pair. That is a
// format gap, recorded in docs/tasks/hub-em-s3-icebug-e-lancedb.md, and it is why an AST context
// still downloads while a knowledge one no longer does.

// MountedWiki is a published wiki index, addressed on object storage.
type MountedWiki struct {
	// Config is what opens it. The URI ends in the index directory, so the engine reads the
	// objects in place.
	Config lancestore.Config
	// ArtifactID and Version identify what is mounted, for messages that have to name it.
	ArtifactID string
	Version    string
}

// MountedWikiFor resolves the mounted index of a knowledge context, if there is one.
//
// It returns false — not an error — for every legitimate reason a context is not mounted: it is a
// local import, it is a linked sibling, or the Hub is not configured on this machine. Those are
// states, not faults, and the caller falls back to the local directory.
func (s *S3Store) MountedWikiFor(projectDir, contextName string) (MountedWiki, bool) {
	if s == nil || !s.Configured() {
		return MountedWiki{}, false
	}
	rec, ok := store.LookupContext(projectDir, store.KindKnowledge, contextName)
	if !ok || !rec.IsHub() {
		return MountedWiki{}, false
	}
	return s.MountedWikiAt(rec.ArtifactID, rec.Version, rec.ProjectID)
}

// MountedWikiAt builds the mount for an artifact named directly, which is what publishing and
// installing use before any context record exists.
func (s *S3Store) MountedWikiAt(artifactID, version, projectID string) (MountedWiki, bool) {
	if s == nil || !s.Configured() || version == "" {
		return MountedWiki{}, false
	}
	uri := s.ArtifactURI(TypeKnowledge, artifactID, version, projectID, wiki.WikiIndexDirName)
	if uri == "" {
		return MountedWiki{}, false
	}
	return MountedWiki{
		Config:     lancestore.Config{URI: uri, S3: s.cfg},
		ArtifactID: artifactID,
		Version:    version,
	}, true
}
