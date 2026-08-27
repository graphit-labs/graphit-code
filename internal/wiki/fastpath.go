package wiki

import (
	"context"
	"os"
	"path/filepath"
)

// Fast-path helper — shared by wiki generators

// DocHashEntry is the input unit for FastPathCheck.
// Each entry represents one source document that will produce wiki pages.
type DocHashEntry struct {
	// CacheKey is the key used in the WikiProcessCache (relPath for knowledge,
	// filename for memory).
	CacheKey string
	// ContentHash is the content hash of the source document.
	ContentHash string
	// Slug is the wiki page slug generated for this document (used to check
	// for deletions by comparing against wikiDir contents).
	Slug string
}

// FastPathCheck reports whether the wiki is already up-to-date and the full
// generation pipeline can be skipped.
//
// It returns true when ALL of the following hold:
//  1. wiki.db already exists in wikiDir.
//  2. processCache is non-nil and every entry's content hash is unchanged
//     according to the cache manifest (O(1) per entry — no disk reads).
//  3. The set of .md pages in wikiDir matches the set of entry slugs EXACTLY —
//     no page was deleted, and no entry is still missing its page.
//
// When FastPathCheck returns true the caller should call processCache.Save()
// and return early without regenerating any pages or rebuilding the DB.
//
// SAFETY: the slug comparison in (3) must stay bidirectional. The cache is
// populated by the same generation pass that then calls this function, so on a
// virgin wikiDir condition (2) is vacuously satisfied — only the missing pages
// reveal that nothing has been generated yet.
func FastPathCheck(ctx context.Context, wikiDir string, entries []DocHashEntry, cache *WikiProcessCache) bool {
	if cache == nil {
		return false
	}

	// (1) The DB must already exist AND hold something.
	//
	// Existence alone is what this used to ask, and it left a state the fast path could
	// never escape: a store that is present but empty satisfies every other condition —
	// the hashes still match because the sources did not change, and the pages are all on
	// disk because they were generated — so generation is skipped, the DB is never built,
	// and every later run skips again. `memory index` reported "complete" in 0.0s over a
	// 16 KB store with 152 pages beside it, and search silently fell back to scanning the
	// markdown.
	//
	// Two different bugs landed in that state, which is why the check is here rather than
	// at either of their sites: a pre-migration SQLite file that could not be opened at
	// all, and a rebuild whose error was discarded. The fast path is where they became
	// permanent.
	//
	// This costs one store open per generation — not per file — on a path whose whole
	// purpose is to be nearly free. That is the deliberate trade: the alternative is a
	// check on file SIZE, which needs a magic threshold and would go wrong quietly the
	// first time an empty store's footprint changes.
	if !IndexHasContent(ctx, wikiDir) {
		return false
	}

	// (2) Check that every source file is cached at its current hash.
	newSlugs := make(map[string]bool, len(entries))
	for _, e := range entries {
		newSlugs[e.Slug] = true
		if e.ContentHash == "" {
			return false
		}
		// HasChanged is O(1) — reads from the in-memory manifest.
		if cache.HasChanged(e.CacheKey, e.ContentHash) {
			return false
		}
	}

	// (3) The pages on disk must correspond exactly to the current entries.
	existing, err := os.ReadDir(wikiDir)
	if err != nil {
		return false
	}
	existingSlugs := make(map[string]bool, len(existing))
	for _, f := range existing {
		if f.IsDir() {
			continue
		}
		name := f.Name()
		if filepath.Ext(name) != ".md" {
			continue
		}
		existingSlugs[name[:len(name)-3]] = true // trim ".md"
	}
	if len(existingSlugs) != len(newSlugs) {
		return false
	}
	for slug := range newSlugs {
		if !existingSlugs[slug] {
			return false
		}
	}

	return true
}

// IndexHasContent reports whether wikiDir holds an index worth skipping a rebuild for.
//
// It asks whether the store has ROWS, not whether the file exists, and that distinction is
// the whole reason it exists. Both skip gates — StatPreCheck and FastPathCheck — used to
// end in `os.Stat(wiki.db) == nil`, which produces a state neither can escape: a store that
// is present but empty satisfies every other condition, because the sources did not change
// and the pages were generated. Generation is skipped, the index is never built, and every
// later run skips again.
//
// Observed: `memory index` reporting "complete" in 0.0 s over a 16 KB store with 152 pages
// beside it, while search silently fell back to scanning the markdown. Two unrelated bugs
// landed there — a pre-migration SQLite file that could not be opened, and a rebuild whose
// error was discarded — which is why the check belongs at the gate rather than at either
// site: the gate is what made them permanent.
//
// It costs one store open per generation, not per file. The alternative was a threshold on
// file size, which needs a magic number and fails quietly the first time an empty store's
// footprint changes.
func IndexHasContent(ctx context.Context, wikiDir string) bool {
	if _, err := os.Stat(WikiIndexPath(wikiDir)); err != nil {
		return false
	}
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return false
	}
	defer func() { _ = db.Close() }()
	return db.HasContent(ctx)
}
