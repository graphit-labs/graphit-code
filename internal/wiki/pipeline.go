package wiki

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// RebuildDB is the unified WikiDB rebuild lifecycle shared by all LLM wiki
// consumers (knowledge, memory, etc.). It:
//
//  1. Opens (or creates) the WikiDB at wikiDir
//  2. Loads embedding cache from WikiProcessCache shards
//  3. Rebuilds the DB with chunks, cross-references, and sync log
//  4. Exports embeddings back to shards for persistence across rebuilds
//
// Both knowledge and memory wikis call this — the embedding lifecycle,
// FTS5 indexing, and shard management are inherent to the wiki engine.
func RebuildDB(ctx context.Context, wikiDir string, chunks []WikiChunk, xrefs map[string][]string, logEntry *SyncLogEntry, cache *WikiProcessCache) error {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return fmt.Errorf("open wiki db: %w", err)
	}
	defer db.Close()

	// Fast path: if every chunk hash already exists in the DB with the same
	// count, skip the expensive write-to-temp + FTS5 rebuild + atomic rename
	// cycle entirely. This check is a single SELECT query — negligible cost.
	// Note: we only skip when there's no new sync log entry (i.e., no docs
	// actually changed at the higher level).
	if logEntry == nil && db.CheckAllHashesMatch(ctx, chunks) {
		// DB is already in sync — nothing to do.
		return nil
	}

	var embCache EmbeddingCache
	if cache != nil {
		embCache = cache.LoadAllEmbeddings()
	}

	if err := db.Rebuild(ctx, chunks, xrefs, logEntry, embCache); err != nil {
		return fmt.Errorf("rebuild wiki db: %w", err)
	}

	if cache != nil {
		cache.ExportAllEmbeddingsFromDB(ctx, db)
		_ = cache.Save()
	}
	return nil
}

// ResetDir empties a wiki directory and returns it ready to receive a fresh copy.
//
// It clears rather than merges because placing a published wiki over an existing one
// is additive: a page its publisher deleted would otherwise survive in every consumer
// forever, answering searches with documentation that no longer exists upstream.
func ResetDir(dir string) (string, error) {
	if dir == "" {
		return "", fmt.Errorf("wiki directory is required")
	}
	if err := os.RemoveAll(dir); err != nil {
		return "", fmt.Errorf("clearing wiki %s: %w", dir, err)
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", fmt.Errorf("creating wiki %s: %w", dir, err)
	}
	return dir, nil
}

// BuildDBFromCache builds a wiki's search index from its shards alone, with the
// source documents nowhere on the machine.
//
// This is how a published wiki is installed. The producer compiled it once and
// shipped the shards — chunks and embedding vectors, content-addressed — so the
// consumer has no reason to run the generator again, and every reason not to:
// re-deriving the pages would need the sources, and re-deriving the vectors would
// run the embedding model over text whose vectors are already in hand.
//
// It reports how many chunks it indexed. Zero means the shards carried nothing
// usable, which is a real answer and not an error: an artifact can be published
// empty, and a caller that treats zero as success would leave a healthy, empty
// index behind — the failure this package works hardest to make impossible.
func BuildDBFromCache(ctx context.Context, wikiDir string) (int, error) {
	cache, err := NewWikiProcessCache(wikiDir)
	if err != nil {
		return 0, fmt.Errorf("open wiki cache at %s: %w", wikiDir, err)
	}
	defer func() { _ = cache.Close() }()

	chunks, xrefs := cache.LoadAllChunks()
	if len(chunks) == 0 {
		return 0, nil
	}

	// A log entry marks the index as freshly built, which also stops RebuildDB's
	// hash fast path from deciding there is nothing to do against an empty DB.
	logEntry := &SyncLogEntry{
		Timestamp:       time.Now().UTC().Format(time.RFC3339),
		TotalDocs:       len(chunks),
		ArticlesWritten: len(chunks),
	}
	if err := RebuildDB(ctx, wikiDir, chunks, xrefs, logEntry, cache); err != nil {
		return 0, err
	}
	return len(chunks), nil
}

// IsDerivedFile reports whether an entry in a wiki directory is rebuildable from the
// shards beside it, and therefore not worth carrying anywhere.
//
// Only the index is: it is the largest thing in the directory and BuildDBFromCache
// reconstructs it in seconds. The shards are not derived in any sense that matters
// here — they are what the index is rebuilt FROM.
//
// 🔒 IT MATCHES ANY COMPONENT OF THE PATH, NOT JUST THE LAST ONE, and that is the whole
// correctness of it. The index used to be a single file, so testing `filepath.Base` was
// enough; it is a DIRECTORY now, and a base-only test says false for everything inside it
// — `index.lance/chunks.lance/_indices/…/part_0_invert.lance` has base `part_0_invert.lance`.
// Callers walk recursively and ask per entry, so a base-only answer published the entire
// index: every FTS fragment, every transaction log, every version manifest, in the exact
// artifact whose whole point was to leave it behind.
//
// This shipped, and it was invisible because the test that guarded it asserted the absence
// of `wiki.db` — true for the wrong reason once the name changed. Assert against
// WikiIndexDirName, never a literal.
func IsDerivedFile(rel string) bool {
	for _, part := range strings.FieldsFunc(rel, func(r rune) bool { return r == '/' || r == filepath.Separator }) {
		part = strings.ToLower(part)
		if part == WikiIndexDirName || strings.HasPrefix(part, WikiIndexDirName+"-") {
			return true
		}
	}
	return false
}

// ContentHash computes a truncated SHA256 hash for content deduplication.
// Used by all wiki consumers for incremental cache tracking.
func ContentHash(data []byte) string {
	sum := sha256.Sum256(data)
	return fmt.Sprintf("%x", sum)[:16]
}
