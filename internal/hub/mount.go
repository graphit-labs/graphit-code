package hub

import (
	"context"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

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
func (s *S3Store) MountedWikiFor(ctx context.Context, projectDir, contextName string) (MountedWiki, bool, error) {
	if s == nil || !s.Configured() {
		return MountedWiki{}, false, nil
	}
	rec, ok := store.LookupContext(projectDir, store.KindKnowledge, contextName)
	if !ok || !rec.IsHub() {
		return MountedWiki{}, false, nil
	}
	return s.MountedWikiAt(ctx, rec.ArtifactID, rec.Version, rec.ProjectID)
}

// MountedWikiAt builds the mount for an artifact named directly, which is what publishing and
// installing use before any context record exists.
func (s *S3Store) MountedWikiAt(ctx context.Context, artifactID, version, projectID string) (MountedWiki, bool, error) {
	if s == nil || !s.Configured() || version == "" {
		return MountedWiki{}, false, nil
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return MountedWiki{}, false, err
	}
	uri := s.ArtifactURI(TypeKnowledge, artifactID, version, projectID, wiki.WikiIndexDirName)
	if uri == "" {
		return MountedWiki{}, false, nil
	}
	return MountedWiki{
		Config:     lancestore.Config{URI: uri, S3: s.cfg},
		ArtifactID: artifactID,
		Version:    version,
	}, true, nil
}
