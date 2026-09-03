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
	_, err := rebuildIcebugFromCacheWithDeltaTimed(ctx, cache, changed, deleted, cluster, rootPath, logger, isIncremental, finalDir, reverseEdges)
	return err
}

type icebugRebuildTiming struct {
	Prepare time.Duration
	Export  time.Duration
	Publish time.Duration
}

func rebuildIcebugFromCacheWithDeltaTimed(ctx context.Context, cache *ShardCache, changed, deleted []string, cluster, rootPath string, logger *slog.Logger, isIncremental bool, finalDir string, reverseEdges bool) (icebugRebuildTiming, error) {
	return rebuildIcebugFromPreparedWithDeltaTimed(ctx, cache, nil, changed, deleted, cluster, rootPath, logger, isIncremental, finalDir, reverseEdges)
}

func rebuildIcebugFromPreparedWithDeltaTimed(ctx context.Context, cache *ShardCache, prepared map[string]*parseCacheEntry, changed, deleted []string, cluster, rootPath string, logger *slog.Logger, isIncremental bool, finalDir string, reverseEdges bool) (icebugRebuildTiming, error) {
	timing, publication, err := rebuildIcebugFromPreparedWithDeltaStagedTimed(ctx, cache, prepared, changed, deleted, cluster, rootPath, logger, isIncremental, finalDir, reverseEdges)
	if err == nil && publication != nil {
		publication.Commit()
	}
	return timing, err
}

type icebugPublication struct {
	finalDir  string
	backupDir string
	active    bool
}

func (p *icebugPublication) Commit() {
	if p == nil || !p.active {
		return
	}
	if p.backupDir != "" {
		_ = os.RemoveAll(p.backupDir)
	}
	p.active = false
}

func (p *icebugPublication) Rollback() error {
	if p == nil || !p.active {
		return nil
	}
	p.active = false
	if p.backupDir == "" {
		return os.RemoveAll(p.finalDir)
	}

	discardDir := p.finalDir + ".discard." + shortHex()
	if err := os.Rename(p.finalDir, discardDir); err != nil {
		return fmt.Errorf("move failed graph publication aside: %w", err)
	}
	if err := os.Rename(p.backupDir, p.finalDir); err != nil {
		_ = os.Rename(discardDir, p.finalDir)
		return fmt.Errorf("restore previous graph publication: %w", err)
	}
	_ = os.RemoveAll(discardDir)
	return nil
}

func rebuildIcebugFromPreparedWithDeltaStagedTimed(ctx context.Context, cache *ShardCache, prepared map[string]*parseCacheEntry, changed, deleted []string, cluster, rootPath string, logger *slog.Logger, isIncremental bool, finalDir string, reverseEdges bool) (icebugRebuildTiming, *icebugPublication, error) {
	var timing icebugRebuildTiming
	log := slogutil.Resolve(logger)
	if cache == nil || cache.Count() == 0 {
		return timing, nil, nil
	}

	t0 := time.Now()
	entries := prepared
	if entries == nil {
		entries = make(map[string]*parseCacheEntry, cache.Count())
		cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
			entries[relPath] = entry
			return true
		})
	}
	ri := newRebuildIndex(entries, targetRulesFor(rootPath))

	tmpDir := finalDir + ".tmp." + shortHex()
	_ = os.RemoveAll(tmpDir)
	removeStaleBundleTemps(finalDir)
	if err := os.MkdirAll(filepath.Dir(finalDir), 0o755); err != nil {
		return timing, nil, fmt.Errorf("icebug rebuild: mkdir final parent: %w", err)
	}
	storageURI := finalDir
	if abs, err := filepath.Abs(finalDir); err == nil {
		storageURI = abs
	}
	doIncremental := isIncremental && len(changed)+len(deleted) > 0 && len(changed)+len(deleted) < cache.Count()/5
	if doIncremental {
		if _, err := os.Stat(finalDir); err != nil {
			doIncremental = false
		}
	}
	var man *ladybug.CanonicalManifest
	var err error
	timing.Prepare = time.Since(t0)
	t0 = time.Now()
	if doIncremental {
		man, err = ExportDirectIncrementalWithReverse(ri, tmpDir, finalDir, storageURI, changed, deleted, reverseEdges)
	} else {
		man, err = ExportDirectFromRebuildIndexWithReverse(ri, tmpDir, storageURI, reverseEdges)
	}
	timing.Export = time.Since(t0)
	if err != nil {
		_ = os.RemoveAll(tmpDir)
		return timing, nil, fmt.Errorf("icebug rebuild direct: %w", err)
	}
	t0 = time.Now()
	publication := &icebugPublication{finalDir: finalDir, active: true}
	if _, statErr := os.Stat(finalDir); statErr == nil {
		publication.backupDir = finalDir + ".backup." + shortHex()
		if err := os.Rename(finalDir, publication.backupDir); err != nil {
			_ = os.RemoveAll(tmpDir)
			return timing, nil, fmt.Errorf("icebug rebuild: back up active bundle: %w", err)
		}
	} else if !os.IsNotExist(statErr) {
		_ = os.RemoveAll(tmpDir)
		return timing, nil, fmt.Errorf("icebug rebuild: inspect active bundle: %w", statErr)
	}
	if err := os.Rename(tmpDir, finalDir); err != nil {
		_ = os.RemoveAll(tmpDir)
		if publication.backupDir != "" {
			_ = os.Rename(publication.backupDir, finalDir)
		}
		return timing, nil, fmt.Errorf("icebug rebuild: rename bundle: %w", err)
	}
	timing.Publish = time.Since(t0)
	log.Info("icebug bundle rebuilt direct", "dir", finalDir, "nodes", len(man.NodeTables), "edges", man.EdgeCount, "incremental", doIncremental)
	return timing, publication, nil
}

const staleBundleTempAge = time.Hour

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
