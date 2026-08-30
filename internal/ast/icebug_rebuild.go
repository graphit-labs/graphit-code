package ast

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

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
func RebuildIcebugFromCache(ctx context.Context, cache *ShardCache, cluster, rootPath string, logger *slog.Logger, finalDir string) error {
	return RebuildIcebugFromCacheWithReverse(ctx, cache, cluster, rootPath, logger, finalDir, false)
}

// RebuildIcebugFromCacheWithReverse is RebuildIcebugFromCache with an explicit
// reverse-edge policy. Callers from the CLI/daemon wire it from hub config.
func RebuildIcebugFromCacheWithReverse(ctx context.Context, cache *ShardCache, cluster, rootPath string, logger *slog.Logger, finalDir string, reverseEdges bool) error {
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
	removeStaleBundleTemps(finalDir)
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

// staleBundleTempAge is how long a working directory must have been untouched before this
// treats it as abandoned.
//
// SAFETY: this guard is the whole correctness of the sweep. A run's working directory is named
// by a random suffix, so a sweep by prefix alone cannot tell one left by a dead run from one a
// CONCURRENT run is writing into right now — and the daemon indexes the same store the CLI
// does. Deleting a live one mid-export does not fail loudly: the export keeps writing into a
// directory that no longer exists, recreates part of it, and publishes the partial result by
// rename. OBSERVED: a bundle went from 549 Parquet files to 175 that way.
//
// An export in flight touches its directory continuously, so any age comfortably above the
// longest export separates the two. A full index of 38k files measured ~16 minutes.
const staleBundleTempAge = time.Hour

// removeStaleBundleTemps drops the working directories a previous run left behind.
//
// Each run names its own by a fresh random suffix, so a run that died before its cleanup — or
// one that errored out of a path that did not clean up — leaves a directory nothing will ever
// look for again. They are invisible individually and unbounded in aggregate.
func removeStaleBundleTemps(finalDir string) {
	parent := filepath.Dir(finalDir)
	entries, err := os.ReadDir(parent)
	if err != nil {
		return
	}
	prefix := filepath.Base(finalDir) + ".tmp."
	cutoff := time.Now().Add(-staleBundleTempAge)
	for _, e := range entries {
		if !e.IsDir() || !strings.HasPrefix(e.Name(), prefix) {
			continue
		}
		info, err := e.Info()
		if err != nil || info.ModTime().After(cutoff) {
			continue
		}
		_ = os.RemoveAll(filepath.Join(parent, e.Name()))
	}
}
