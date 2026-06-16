package wiki

import (
	"crypto/sha256"
	"fmt"
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
func RebuildDB(wikiDir string, chunks []WikiChunk, xrefs map[string][]string, logEntry *SyncLogEntry, cache *WikiProcessCache) error {
	db, err := OpenWikiDB(wikiDir)
	if err != nil {
		return fmt.Errorf("open wiki db: %w", err)
	}
	defer db.Close()

	var embCache EmbeddingCache
	if cache != nil {
		embCache = cache.LoadAllEmbeddings()
	}

	if err := db.Rebuild(chunks, xrefs, logEntry, embCache); err != nil {
		return fmt.Errorf("rebuild wiki db: %w", err)
	}

	if cache != nil {
		cache.ExportAllEmbeddingsFromDB(db)
		_ = cache.Save()
	}
	return nil
}

// ContentHash computes a truncated SHA256 hash for content deduplication.
// Used by all wiki consumers for incremental cache tracking.
func ContentHash(data []byte) string {
	sum := sha256.Sum256(data)
	return fmt.Sprintf("%x", sum)[:16]
}
