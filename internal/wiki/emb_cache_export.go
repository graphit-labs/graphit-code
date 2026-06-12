package wiki

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

const embCacheFilename = "wiki_emb_cache.json"

// PortableEmbCache is the legacy on-disk format for monolithic embedding caches.
// New code uses per-file .emb.json shards via WikiProcessCache instead.
type PortableEmbCache struct {
	Version int               `json:"version"`
	Entries map[string]string `json:"entries"` // content_hash → base64-encoded float32 blob
}

const portableEmbCacheVersion = 1

// ExportEmbeddingCache exports all embeddings to a portable JSON cache file.
// DEPRECATED: New code should use WikiProcessCache.ExportAllEmbeddingsFromDB
// which stores per-file .emb.json shards. This is kept for backward compatibility.
func (w *WikiDB) ExportEmbeddingCache(destDir string) (int, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	rows, err := w.db.Query(`
		SELECT c.content_hash, v.embedding
		FROM chunks_vec_map m
		JOIN chunks c ON c.id = m.chunk_id
		JOIN chunks_vec v ON v.rowid = m.vec_rowid
		WHERE c.content_hash != ''`)
	if err != nil {
		return 0, fmt.Errorf("export query: %w", err)
	}
	defer func() { _ = rows.Close() }()

	cache := &PortableEmbCache{
		Version: portableEmbCacheVersion,
		Entries: make(map[string]string),
	}

	for rows.Next() {
		var hash string
		var blob []byte
		if err := rows.Scan(&hash, &blob); err != nil {
			continue
		}
		if hash != "" && len(blob) > 0 {
			cache.Entries[hash] = base64.StdEncoding.EncodeToString(blob)
		}
	}

	if len(cache.Entries) == 0 {
		return 0, nil
	}

	outPath := filepath.Join(destDir, embCacheFilename)
	data, err := json.Marshal(cache)
	if err != nil {
		return 0, fmt.Errorf("marshal cache: %w", err)
	}

	if err := os.WriteFile(outPath, data, 0o644); err != nil {
		return 0, fmt.Errorf("write cache: %w", err)
	}

	return len(cache.Entries), nil
}

// ImportEmbeddingCache loads embeddings from a portable cache file
// and restores vectors for chunks whose content_hash matches.
// Returns the number of embeddings successfully restored.
func (w *WikiDB) ImportEmbeddingCache(srcDir string) (int, error) {
	w.mu.Lock()
	defer w.mu.Unlock()

	cachePath := filepath.Join(srcDir, embCacheFilename)
	data, err := os.ReadFile(cachePath)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil
		}
		return 0, fmt.Errorf("read cache: %w", err)
	}

	var cache PortableEmbCache
	if err := json.Unmarshal(data, &cache); err != nil {
		return 0, fmt.Errorf("unmarshal cache: %w", err)
	}
	if cache.Version != portableEmbCacheVersion || len(cache.Entries) == 0 {
		return 0, nil
	}

	// Find chunks without embeddings that have a matching content_hash.
	rows, err := w.db.Query(`
		SELECT c.id, c.slug, c.title, c.summary, c.content_hash
		FROM chunks c
		LEFT JOIN chunks_vec_map m ON m.chunk_id = c.id
		WHERE m.chunk_id IS NULL AND c.content_hash != '' AND c.word_count >= 10`)
	if err != nil {
		return 0, fmt.Errorf("query pending: %w", err)
	}
	defer func() { _ = rows.Close() }()

	type restoreRow struct {
		id      int
		slug    string
		title   string
		summary string
		hash    string
	}
	var pending []restoreRow
	for rows.Next() {
		var r restoreRow
		if err := rows.Scan(&r.id, &r.slug, &r.title, &r.summary, &r.hash); err != nil {
			continue
		}
		if _, ok := cache.Entries[r.hash]; ok {
			pending = append(pending, r)
		}
	}

	restored := 0
	for _, r := range pending {
		b64 := cache.Entries[r.hash]
		blob, err := base64.StdEncoding.DecodeString(b64)
		if err != nil || len(blob) == 0 {
			continue
		}

		res, err := w.db.Exec(`INSERT INTO chunks_vec(embedding) VALUES (?)`, blob)
		if err != nil {
			continue
		}
		rowID, _ := res.LastInsertId()
		_, err = w.db.Exec(`INSERT OR REPLACE INTO chunks_vec_map(chunk_id, vec_rowid, slug, title, summary) VALUES (?, ?, ?, ?, ?)`,
			r.id, rowID, r.slug, r.title, r.summary)
		if err != nil {
			continue
		}
		restored++
	}

	return restored, nil
}

// EmbCachePath returns the expected path of the legacy embedding cache file for a wiki dir.
func EmbCachePath(wikiDir string) string {
	return filepath.Join(wikiDir, embCacheFilename)
}

// LoadEmbeddingCache reads the legacy monolithic JSON cache file and returns
// an EmbeddingCache for use in Rebuild(). Returns nil if the file doesn't exist.
// For new code, prefer WikiProcessCache.LoadAllEmbeddings() which reads per-file shards.
func LoadEmbeddingCache(wikiDir string) EmbeddingCache {
	cachePath := filepath.Join(wikiDir, embCacheFilename)
	data, err := os.ReadFile(cachePath)
	if err != nil {
		return nil
	}

	var portable PortableEmbCache
	if err := json.Unmarshal(data, &portable); err != nil {
		return nil
	}
	if portable.Version != portableEmbCacheVersion || len(portable.Entries) == 0 {
		return nil
	}

	cache := make(EmbeddingCache, len(portable.Entries))
	for hash, b64 := range portable.Entries {
		blob, err := base64.StdEncoding.DecodeString(b64)
		if err != nil || len(blob) == 0 {
			continue
		}
		cache[hash] = blob
	}
	return cache
}
