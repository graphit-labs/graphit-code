package wiki

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
)

// SyncDB is the unified incremental WikiDB lifecycle shared by knowledge and memory.
// The table itself supplies the previous rows and cached embeddings; no sidecar cache exists.
func SyncDB(ctx context.Context, wikiDir string, chunks []WikiChunk, xrefs map[string][]string, logEntry *SyncLogEntry) error {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return fmt.Errorf("open wiki db: %w", err)
	}
	defer db.Close()

	if err := db.Sync(ctx, chunks, xrefs, logEntry); err != nil {
		return fmt.Errorf("sync wiki db: %w", err)
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

// ContentHash computes a truncated SHA256 hash for content deduplication.
// Used by all wiki consumers for incremental cache tracking.
func ContentHash(data []byte) string {
	sum := sha256.Sum256(data)
	return fmt.Sprintf("%x", sum)[:16]
}
