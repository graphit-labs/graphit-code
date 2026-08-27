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
// Only the database is: it is the largest thing in the directory and BuildDBFromCache
// reconstructs it in seconds. The pages and the shards are not derived in any sense
// that matters here — the pages are what a reader opens, and the shards are what the
// database is rebuilt FROM.
func IsDerivedFile(rel string) bool {
	base := strings.ToLower(filepath.Base(rel))
	if base == WikiIndexDirName || strings.HasPrefix(base, WikiIndexDirName+"-") {
		return true
	}
	// The SQLite index this replaced, and its write-ahead log siblings.
	//
	// STILL EXCLUDED even though nothing reads them any more, and that is the point: every
	// machine that indexed before the change has a wiki.db sitting in its wiki directory, and
	// the alternative to naming it here is that the wiki INDEXES ITS OWN OLD DATABASE as a
	// source document. Nothing deletes it — this project keeps no migration — so nothing else
	// would stop that.
	return base == legacySQLiteIndexName || strings.HasPrefix(base, legacySQLiteIndexName+"-")
}

// legacySQLiteIndexName is what the wiki index was called when it was a SQLite file.
const legacySQLiteIndexName = "wiki.db"

// ContentHash computes a truncated SHA256 hash for content deduplication.
// Used by all wiki consumers for incremental cache tracking.
func ContentHash(data []byte) string {
	sum := sha256.Sum256(data)
	return fmt.Sprintf("%x", sum)[:16]
}
