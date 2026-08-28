package ast

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// RebuildIcebugFromCache builds the local icebug filesystem bundle directly from shards.
// No intermediate Ladybug DB is populated – Parquets are written directly from RebuildIndex.
// If finalDir exists and changed/deleted is small, only affected Parquets are rewritten.
//
// finalDir is the target bundle directory. It is the backend's own IcebugDir in every
// production path — a project resolves it from `store.ASTProjectIcebugDir(projectDir)`,
// an imported context from its context dir — and taking it as a parameter keeps the
// pipeline usable against a store the caller chose rather than one re-derived from rootPath.
func RebuildIcebugFromCache(ctx context.Context, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger, finalDir string) error {
	return RebuildIcebugFromCacheWithReverse(ctx, cache, embCache, cluster, rootPath, logger, finalDir, false)
}

// RebuildIcebugFromCacheWithReverse is RebuildIcebugFromCache with an explicit
// reverse-edge policy. Callers from the CLI/daemon wire it from hub config.
func RebuildIcebugFromCacheWithReverse(ctx context.Context, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger, finalDir string, reverseEdges bool) error {
	return rebuildIcebugFromCacheWithDelta(ctx, cache, nil, nil, cluster, rootPath, logger, false, finalDir, reverseEdges)
}

func rebuildIcebugFromCacheWithDelta(ctx context.Context, cache *ShardCache, changed, deleted []string, cluster, rootPath string, logger *slog.Logger, isIncremental bool, finalDir string, reverseEdges bool) error {
	log := slogutil.Resolve(logger)
	if cache == nil || cache.Count() == 0 {
		return nil
	}
	entries := make(map[string]*parseCacheEntry, cache.Count())
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		entries[relPath] = entry
		return true
	})
	ri := newRebuildIndex(entries, targetRulesFor(rootPath))

	tmpDir := finalDir + ".tmp." + shortHex()
	_ = os.RemoveAll(tmpDir)
	if err := os.MkdirAll(filepath.Dir(finalDir), 0o755); err != nil {
		return fmt.Errorf("icebug rebuild: mkdir final parent: %w", err)
	}
	storageURI := finalDir
	if abs, err := filepath.Abs(finalDir); err == nil {
		storageURI = abs
	}
	// Determine if incremental is possible and beneficial
	doIncremental := isIncremental && len(changed)+len(deleted) > 0 && len(changed)+len(deleted) < cache.Count()/5
	if doIncremental {
		if _, err := os.Stat(finalDir); err != nil {
			doIncremental = false
		}
	}
	var man *ladybug.CanonicalManifest
	var err error
	if doIncremental {
		man, err = ExportDirectIncrementalWithReverse(ri, tmpDir, finalDir, storageURI, changed, deleted, reverseEdges)
	} else {
		man, err = ExportDirectFromRebuildIndexWithReverse(ri, tmpDir, storageURI, reverseEdges)
	}
	if err != nil {
		_ = os.RemoveAll(tmpDir)
		return fmt.Errorf("icebug rebuild direct: %w", err)
	}
	_ = os.RemoveAll(finalDir)
	if err := os.Rename(tmpDir, finalDir); err != nil {
		_ = os.RemoveAll(tmpDir)
		return fmt.Errorf("icebug rebuild: rename bundle: %w", err)
	}
	log.Info("icebug bundle rebuilt direct", "dir", finalDir, "nodes", len(man.NodeTables), "edges", man.EdgeCount, "incremental", doIncremental)
	return nil
}


