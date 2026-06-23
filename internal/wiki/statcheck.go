package wiki

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

// StatPreCheck is the AST-style mtime+size pre-check for wiki generators.
// It is the shared implementation used by both knowledge and memory wikis.
//
// How it works (mirrors the AST pipeline exactly):
//
//   - Phase A: stat all cached files in parallel.
//     Files whose mtime+size match the cache are considered unchanged.
//     Files with a different stat go to Phase B.
//
//   - Phase B: ReadFile + SHA-256 only the Phase A misses.
//     If the content hash is unchanged, update the cached mtime so the
//     next sync skips them via Phase A.  If the hash changed, return false.
//
//   - If every file is unchanged and wiki.db exists: return true.
//     The caller can return early without rebuilding anything.
//
// Parameters:
//   - baseDir: directory from which cache RelPaths are resolved
//     (project root for knowledge, rawDir for memory).
//   - wikiDir: directory containing wiki.db and the generated .md pages.
//   - cache:   the WikiProcessCache for this wiki.
//
// Returns true when the caller can safely skip the full rebuild.
func StatPreCheck(baseDir, wikiDir string, cache *WikiProcessCache, watchFiles ...string) bool {
	if cache == nil {
		return false
	}

	cachedEntries := cache.AllStatEntries()
	if len(cachedEntries) == 0 {
		return false
	}

	for _, wf := range watchFiles {
		absWf := wf
		if !filepath.IsAbs(absWf) {
			absWf = filepath.Join(baseDir, wf)
		}
		info, err := os.Stat(absWf)
		if err != nil {
			continue
		}
		if cache.WatchFileChanged(wf, info.ModTime().UnixNano(), info.Size()) {
			return false
		}
	}

	// Phase A: stat all cached files in parallel.
	type statEntry struct {
		ce      CachedStatEntry
		matched bool
	}
	resultCh := make(chan statEntry, len(cachedEntries))
	var wg sync.WaitGroup
	sem := make(chan struct{}, 32)
	for _, ce := range cachedEntries {
		wg.Add(1)
		go func(ce CachedStatEntry) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()
			info, err := os.Stat(filepath.Join(baseDir, ce.RelPath))
			matched := err == nil &&
				info.ModTime().UnixNano() == ce.Mtime &&
				info.Size() == ce.Size
			resultCh <- statEntry{ce, matched}
		}(ce)
	}
	wg.Wait()
	close(resultCh)

	// Partition into stat-match and need-hash.
	var needHash []CachedStatEntry
	for sr := range resultCh {
		if !sr.matched {
			needHash = append(needHash, sr.ce)
		}
	}

	// Phase B: read+hash only the files whose stat changed.
	// If the hash is unchanged, update mtime so next sync skips this file.
	for _, ce := range needHash {
		absPath := filepath.Join(baseDir, ce.RelPath)
		data, err := os.ReadFile(absPath)
		if err != nil {
			return false // file deleted or unreadable → rebuild
		}
		newHash := fmt.Sprintf("%x", sha256.Sum256(data))[:16]
		if newHash != ce.Hash {
			return false // content actually changed → rebuild
		}
		// Same content — record new mtime so next sync hits Phase A fast path.
		if info, statErr := os.Stat(absPath); statErr == nil {
			cache.StoreMtime(ce.RelPath, info.ModTime().UnixNano(), info.Size())
		}
	}

	// AST pattern: if no source file changed and the wiki DB exists, done.
	_, dbErr := os.Stat(filepath.Join(wikiDir, "wiki.db"))
	return dbErr == nil
}
