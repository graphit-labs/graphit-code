package wiki

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

// StatPreCheckOpts configures StatPreCheck.
type StatPreCheckOpts struct {
	// WatchFiles are extra files whose mtime+size invalidate the pre-check.
	// Relative paths are resolved against baseDir.
	WatchFiles []string

	// CurrentSourceFiles enumerates the cache keys of every source file that
	// exists right now. StatPreCheck only ever walks the cache, so a file the
	// cache has never seen is invisible to it — this is how added files are
	// detected. When nil, additions are NOT detected and the caller is
	// responsible for having another change signal.
	CurrentSourceFiles func() []string
}

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
//   - If every file is unchanged and the index holds content: return true.
//     The caller can return early without rebuilding anything.
//
// Deletions are caught by Phase B (the ReadFile of a vanished file fails).
// Additions are caught only via opts.CurrentSourceFiles.
//
// Parameters:
//   - baseDir: directory from which cache RelPaths are resolved
//     (project root for knowledge, rawDir for memory).
//   - wikiDir: directory holding the wiki's Lance index and its shard cache.
//   - cache:   the WikiProcessCache for this wiki.
//
// Returns true when the caller can safely skip the full rebuild.
func StatPreCheck(ctx context.Context, baseDir, wikiDir string, cache *WikiProcessCache, opts StatPreCheckOpts) bool {
	if cache == nil {
		return false
	}

	cachedEntries := cache.AllStatEntries()
	if len(cachedEntries) == 0 {
		return false
	}

	for _, wf := range opts.WatchFiles {
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

	// A source file the cache has never heard of means the wiki is incomplete,
	// no matter how unchanged every cached file is.
	if opts.CurrentSourceFiles != nil {
		cachedKeys := make(map[string]bool, len(cachedEntries))
		for _, ce := range cachedEntries {
			cachedKeys[ce.RelPath] = true
		}
		current := opts.CurrentSourceFiles()
		if len(current) != len(cachedKeys) {
			return false
		}
		for _, key := range current {
			if !cachedKeys[key] {
				return false
			}
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

	// No source changed. THAT IS NOT ENOUGH, and the difference cost a real staleness bug.
	//
	// This used to end at `IndexHasContent` — the index exists and has rows — which asks nothing
	// about whether those rows correspond to these sources. The two can disagree, and there is a
	// mechanism that makes them disagree PERMANENTLY: the generation pass writes the process cache
	// (hashes and mtimes) BEFORE it rebuilds the index, so anything that stops the pass between
	// those two points — a crash, a cancelled context, or a bug in a later gate — leaves a cache
	// that says "already processed" beside an index that never received the work. On the next run
	// every file stat-matches, no hash is even computed, and this returns true. Forever.
	//
	// Observed exactly that way: a buggy fast path skipped a rebuild while updating the cache, and
	// from then on `knowledge index` reported "0 articles" in 27 ms over an index missing the edit.
	//
	// So the gate is now "the index holds what the cache claims was processed", which is evidence
	// from outside the cache — the same rule the earlier incident in this file's history produced.
	return indexHoldsCachedSources(ctx, wikiDir, cachedEntries)
}

// indexHoldsCachedSources reports whether the compiled index reflects exactly the files the process
// cache says it processed.
//
// The comparison is on the content hashes, because that is what the two sides share: the cache keys
// by source path, the index keys by slug, and `content_hash` is the source hash the row was built
// from. One source document is one row — the per-heading chunker was removed — so the counts are
// directly comparable, and a hash appearing in one and not the other means the index is behind.
//
// Two documents with identical content produce identical hashes, so a duplicate is not a false
// mismatch: the multiset comparison treats them as the interchangeable rows they are.
func indexHoldsCachedSources(ctx context.Context, wikiDir string, cached []CachedStatEntry) bool {
	if !IndexHasContent(ctx, wikiDir) {
		return false
	}
	indexed, err := IndexedPageHashes(ctx, wikiDir)
	if err != nil {
		return false
	}
	if len(indexed) != len(cached) {
		return false
	}
	remaining := make(map[string]int, len(indexed))
	for _, hash := range indexed {
		remaining[hash]++
	}
	for _, e := range cached {
		if e.Hash == "" || remaining[e.Hash] == 0 {
			return false
		}
		remaining[e.Hash]--
	}
	return true
}
