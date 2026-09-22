package hub

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

// ensureASTStore builds the local catalog of a Hub AST version under a cross-process lock.
func (s *HubService) ensureASTStore(ctx context.Context, projectID, version string, artifact artifactRef) (string, error) {
	if err := s.registry.authorizeProject(ctx, artifact.ProjectID); err != nil {
		return "", err
	}
	storeDir := ast.HubContextDir(projectID, version)

	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		return "", fmt.Errorf("creating shared AST store dir: %w", err)
	}

	lock, lockErr := lockfile.Acquire(sharedASTBuildLockPath(storeDir), 2*time.Minute)
	if lockErr == nil {
		defer lock.Release()
	} else {
		if !errors.Is(lockErr, lockfile.ErrLocked) {
			return "", fmt.Errorf("locking shared AST store for %s@%s: %w", projectID, version, lockErr)
		}
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

type artifactRef struct {
	ID        string
	ProjectID string
}

// ResolveASTContext resolves latest or a requested version to one exact remote
// AST context. It stages only the local catalog needed to address the published
// graph and search mount; it does not install the artifact into a project lock.
func (s *HubService) ResolveASTContext(ctx context.Context, projectID, qualifiedID string) (storeDir, version string, err error) {
	realID, requested := qualifiedID, ""
	if parts := strings.SplitN(qualifiedID, "@", 2); len(parts) == 2 {
		realID, requested = parts[0], parts[1]
	}
	entry, err := s.registry.ResolveEntry(ctx, projectID, realID, TypeAST)
	if err != nil {
		return "", "", err
	}
	version, err = resolveEntryVersion(entry, requested)
	if err != nil {
		return "", "", err
	}
	if version == "" {
		return "", "", fmt.Errorf("AST artifact %q has no published version", realID)
	}
	contextID := ast.HubContextID(entry.ProjectID)
	storeDir, err = s.ensureASTStore(ctx, contextID, version, artifactRef{ID: realID, ProjectID: entry.ProjectID})
	return storeDir, version, err
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
	schema, err := st.ReadArtifactFile(ctx, artifact.ProjectID, key)
	if err != nil {
		return fmt.Errorf("mounting the AST context %s@%s: reading the published schema at %s: %w",
			artifact.ID, version, key, err)
	}
	if err := ast.MountIcebugGraph(ctx, storeDir, string(schema), s.Logger); err != nil {
		return err
	}

	manifestKey := s3store.JoinKey(prefix, ast.IcebugBundleDir, ladybug.IcebugManifestFile)
	if manifestRaw, mErr := st.ReadArtifactFile(ctx, artifact.ProjectID, manifestKey); mErr == nil {
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

func (s *HubService) cleanupSharedASTStore(meta *LockfileArtifactMeta) {
	if meta == nil || meta.Version == "" || meta.Version == "local" {
		return
	}
	contextID := ast.HubContextID(meta.ProjectID)
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
	// Use the same stable lock as the builder. In particular, never unlink an
	// in-directory lock inode while a builder still holds an old handle to it.
	lock, err := lockfile.TryAcquire(sharedASTBuildLockPath(storeDir))
	if err != nil {
		s.log().Warn("skipping shared AST store cleanup while build lock is unavailable", "dir", storeDir, "error", err)
		return
	}
	defer lock.Release()
	if err := os.RemoveAll(storeDir); err != nil {
		s.log().Warn("removing shared AST store", "dir", storeDir, "error", err)
		return
	}
	_ = os.Remove(filepath.Dir(storeDir))
}

func sharedASTBuildLockPath(storeDir string) string {
	return filepath.Join(filepath.Dir(storeDir), "."+filepath.Base(storeDir)+".build.lock")
}
