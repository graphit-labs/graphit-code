package wiki

import (
	"context"
	"os"
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

// FastPathCheck reports whether the wiki is already up-to-date and the full generation pipeline
// can be skipped.
//
// It returns true when the INDEX holds exactly the given entries, at exactly the given content
// hashes — no more, no fewer, none stale.
//
// 🔒 IT MUST NOT ASK THE PROCESS CACHE, AND THAT IS THE WHOLE POINT OF THIS FUNCTION'S HISTORY.
//
// It used to, and the check was worthless: the generation pass calls `processCache.Store` with each
// document's NEW hash while assembling the documents, and only then calls this. So by the time the
// question was asked, the cache had already been told the answer — `HasChanged` returned false for
// every entry, including the one the user had just edited. A document could be edited and its
// rebuild skipped, silently, in 110 ms.
//
// That defect was present for as long as the cache check was, and it was INVISIBLE because search
// fell back to scanning the `.md` pages: the pages were rewritten from the fresh sources, so a
// stale index was answered around. Deleting that fallback — which is what made the index the single
// artifact — turned a masked bug into a visible one, and reproduced it on the first edit.
//
// The rule the fix follows is the one the previous incident already wrote down: **the evidence that
// work is unnecessary has to come from OUTSIDE the thing the same pass populates.** The index is
// that outside: it is written by `RebuildDB` at the end of the pass, so what it holds is what the
// LAST pass achieved, and `content_hash` is the source hash the row was built from.
//
// Deletions, additions and edits are therefore all one comparison, rather than three conditions
// that each covered one case:
//
//	an entry the index has never seen   -> a new document
//	an index slug no entry claims       -> a deleted document
//	a hash that differs                 -> an edited document
//
// It costs one store open and one PROJECTED query per generation — not per file — on a path whose
// purpose is to be nearly free. The store open was already being paid for the emptiness check.
func FastPathCheck(ctx context.Context, wikiDir string, entries []DocHashEntry, cache *WikiProcessCache) bool {
	if cache == nil {
		return false
	}

	// An index that is present but EMPTY is a state the fast path used to be unable to escape:
	// every other condition was satisfied — the sources had not changed and the pages were on
	// disk — so generation was skipped, the index was never built, and every later run skipped
	// again. `memory index` reported "complete" in 0.0 s over a 16 KB store with 152 pages beside
	// it. Two unrelated bugs landed in that state, which is why the check is at the gate rather
	// than at either of their sites: the gate is what made them permanent.
	if !IndexHasContent(ctx, wikiDir) {
		return false
	}

	indexed, err := IndexedPageHashes(ctx, wikiDir)
	if err != nil {
		return false
	}
	if len(indexed) != len(entries) {
		return false
	}
	for _, e := range entries {
		if e.ContentHash == "" {
			// A document with no hash cannot be proved unchanged, so it is treated as changed.
			return false
		}
		if indexed[e.Slug] != e.ContentHash {
			return false
		}
	}
	return true
}

// IndexedPageHashes is what a generator compares against to decide what was added, changed and
// deleted: every indexed slug with the content hash it was compiled from.
//
// It replaced an `os.ReadDir` of the wiki directory. The listing answered a weaker question — which
// pages had been WRITTEN — and answering the stronger one meant reading each page's frontmatter for
// its hash. Both are one projected query now.
//
// A wiki with no index yet is not an error: it is a first build, and every document is new.
func IndexedPageHashes(ctx context.Context, wikiDir string) (map[string]string, error) {
	if !IndexHasContent(ctx, wikiDir) {
		return map[string]string{}, nil
	}
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, err
	}
	defer func() { _ = db.Close() }()
	return db.PageHashes(ctx)
}

// IndexHasContent reports whether wikiDir holds an index worth skipping a rebuild for.
//
// It asks whether the store has ROWS, not whether it exists on disk, and that distinction is
// the whole reason it exists. Both skip gates — StatPreCheck and FastPathCheck — used to end
// in a bare existence check, which produces a state neither can escape: a store that is
// present but empty satisfies every other condition, because the sources did not change. The
// rebuild is skipped, the index is never built, and every later run skips again for the same
// reason.
//
// Observed before the fix: `memory index` reporting "complete" in 0.0 s over a 16 KB store
// holding nothing, and search finding no memories in a project that had 152. Two unrelated
// bugs landed in that state — an index file that could not be opened, and a rebuild whose
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
