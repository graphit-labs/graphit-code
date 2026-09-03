package hub

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

// ensureASTStore builds the local catalog of a Hub AST version and returns its directory.
//
// It is idempotent by design, because that idempotence is the whole saving: the second project to
// install the same version finds the catalog already there and pays nothing. A cross-process lock
// guards the build so that two installs racing on the same version do not interleave writes into
// one database — the loser waits and then finds the work done.
//
// cloneDir is accepted and unused. It is kept in the signature because the caller that has one is
// the caller that will stop having one: an artifact published before icebug still has a clone, and
// deleting the parameter now would silently change which of the two paths a stale artifact takes.
func (s *HubService) ensureASTStore(ctx context.Context, cloneDir, projectID, version string, artifact artifactRef) (string, error) {
	_ = cloneDir
	storeDir := ast.HubContextDir(projectID, version)

	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		return "", fmt.Errorf("creating shared AST store dir: %w", err)
	}

	lock, lockErr := lockfile.Acquire(filepath.Join(storeDir, ".build.lock"), 2*time.Minute)
	if lockErr == nil {
		defer lock.Release()
	} else {
		// Waiting elapsed. Building anyway would corrupt whatever the holder is
		// writing, so report rather than race — but a store that is already
		// complete is still usable, which is the common case behind a long build.
		if astStoreBuilt(ctx, storeDir) {
			return storeDir, nil
		}
		return "", fmt.Errorf("shared AST store for %s@%s is being built by another process", projectID, version)
	}

	if astStoreBuilt(ctx, storeDir) {
		return storeDir, nil
	}

	if err := s.mountASTGraph(ctx, artifact, version, storeDir); err != nil {
		return "", err
	}
	return storeDir, nil
}

// artifactRef is the identity the PUBLISHER used, which is not the identity the local store is
// keyed by.
//
// THE TWO WERE CONFLATED AND IT BROKE EVERY INSTALL, in a way no unit test could catch: the local
// directory is keyed by a context id (`<project>-<artifact>`), and the object prefix is keyed by
// the publishing project. Passing the context id as the project produced a prefix nothing had ever
// been written to — `artifacts/ast/t15-demo/3.0.0/` against a publish at
// `artifacts/ast/_global/3.0.0/` — and the failure surfaced as "object not found" on the schema,
// which reads like a corrupt artifact.
//
// The tests could not see it because they called publish and mount with arguments chosen by hand,
// so the two agreed by construction. An end-to-end run through the CLI found it immediately.
type artifactRef struct {
	// ID is the artifact's own id, as published.
	ID string
	// ProjectID is the publishing project, and it is what keys the object prefix. Empty is
	// legitimate and means the global namespace — ArtifactPrefix substitutes its own key.
	ProjectID string
}

// mountASTGraph fetches the published DDL and runs it against a fresh local catalog.
//
// THE GRAPH ITSELF IS NEVER FETCHED. If this ever starts downloading, the whole point is gone —
// which is why the only object it reads is the schema, by name.
func (s *HubService) mountASTGraph(ctx context.Context, artifact artifactRef, version, storeDir string) error {
	st := s.registry.Store()
	if st == nil || !st.Configured() {
		return fmt.Errorf("mounting the AST context: the hub is not configured, so the published " +
			"graph has no location to be read from")
	}

	prefix := ArtifactPrefix(TypeAST, artifact.ID, version, artifact.ProjectID)

	key := s3store.JoinKey(prefix, ast.IcebugBundleDir, ast.IcebugSchemaFile)
	schema, err := st.ReadArtifactFile(ctx, key)
	if err != nil {
		return fmt.Errorf("mounting the AST context %s@%s: reading the published schema at %s: %w",
			artifact.ID, version, key, err)
	}
	if err := ast.MountIcebugGraph(ctx, storeDir, string(schema), s.Logger); err != nil {
		return err
	}

	manifestKey := s3store.JoinKey(prefix, ast.IcebugBundleDir, ladybug.IcebugManifestFile)
	if manifestRaw, mErr := st.ReadArtifactFile(ctx, manifestKey); mErr == nil {
		manifestPath := filepath.Join(storeDir, ladybug.IcebugManifestFile)
		if wErr := os.WriteFile(manifestPath, manifestRaw, 0o644); wErr != nil {
			return fmt.Errorf("mounting the AST context %s@%s: staging %s: %w",
				artifact.ID, version, manifestPath, wErr)
		}
	}

	searchURI := st.ArtifactURI(TypeAST, artifact.ID, version, artifact.ProjectID, ast.SearchBundleDir)
	if searchURI == "" {
		return fmt.Errorf("mounting the AST context %s@%s: the hub produced no location for the "+
			"search index", artifact.ID, version)
	}
	if err := ast.WriteSearchMount(storeDir, searchURI); err != nil {
		return fmt.Errorf("mounting the AST context %s@%s: recording the search index location: %w",
			artifact.ID, version, err)
	}
	return nil
}

func astStoreBuilt(ctx context.Context, storeDir string) bool {
	if _, err := os.Stat(filepath.Join(storeDir, ast.IcebugSchemaFile)); err != nil {
		return false
	}
	return ast.SearchIndexBuilt(ctx, storeDir)
}

func resolveEntryVersion(entry *Entry, reqVersion string) (string, error) {
	constraint, err := ParseVersionConstraint(reqVersion)
	if err != nil {
		return "", fmt.Errorf("invalid version constraint %q: %w", reqVersion, err)
	}
	switch {
	case constraint.IsLatest():
		return entry.Latest, nil
	case len(entry.Versions) > 0:
		resolved, err := ResolveVersion(entry.Versions, constraint)
		if err != nil {
			return "", fmt.Errorf("no version matching %q for %s: %w", reqVersion, entry.ID, err)
		}
		return resolved, nil
	case constraint.IsExact():
		return reqVersion, nil
	default:
		return entry.Latest, nil
	}
}

func (s *HubService) cleanupSharedASTStore(meta *LockfileArtifactMeta, artifactID string) {
	if meta == nil || meta.Version == "" || meta.Version == "local" {
		return
	}
	contextID := ast.HubContextID(meta.ProjectID, artifactID)
	if contextID == "" {
		return
	}
	storeDir := ast.HubContextDir(contextID, meta.Version)
	if !ast.IsUnderHubContextsRoot(storeDir) {
		// Refuse to delete anything the path helpers did not place under the
		// shared root — a malformed project ID must not turn into an rm of a
		// directory somebody cares about.
		return
	}
	if err := os.RemoveAll(storeDir); err != nil {
		s.log().Warn("removing shared AST store", "dir", storeDir, "error", err)
		return
	}
	_ = os.Remove(filepath.Dir(storeDir))
}
