package wiki

import (
	"os"
	"path/filepath"
)

// ---------------------------------------------------------------------------
// Fast-path helper — shared by wiki generators
// ---------------------------------------------------------------------------

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
//  3. No existing wiki .md page has been deleted (i.e., every file in wikiDir
//     corresponds to a current entry slug).
//
// When FastPathCheck returns true the caller should call processCache.Save()
// and return early without regenerating any pages or rebuilding the DB.
func FastPathCheck(wikiDir string, entries []DocHashEntry, cache *WikiProcessCache) bool {
	if cache == nil {
		return false
	}

	// (1) DB must already exist.
	dbPath := filepath.Join(wikiDir, "wiki.db")
	if _, err := os.Stat(dbPath); err != nil {
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

	// (3) Check that no existing wiki page has been deleted.
	existing, err := os.ReadDir(wikiDir)
	if err != nil {
		return false
	}
	for _, f := range existing {
		if f.IsDir() {
			continue
		}
		name := f.Name()
		if filepath.Ext(name) != ".md" {
			continue
		}
		slug := name[:len(name)-3] // trim ".md"
		if !newSlugs[slug] {
			return false
		}
	}

	return true
}
