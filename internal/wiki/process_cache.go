package wiki

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

const wikiProcessCacheVersion = 1

// WikiProcessCache is a JSON-based cache that stores the processed chunks
// and embedding vectors for each source file, keyed by content hash.
// When a source file hasn't changed (same hash), its chunks and embeddings
// are reused from cache instead of re-parsing and re-embedding.
// The cache is the source of truth — wiki.db is always rebuilt from it.
//
// Each source file has two shard files side by side:
//
//	shards/<relPath>.wiki.json  — processed chunks (title, body, summary, etc.)
//	shards/<relPath>.emb.json   — embedding vectors (content_hash → base64 blob)
type WikiProcessCache struct {
	dir      string
	manifest *wikiCacheManifest
	mu       sync.Mutex
	dirty    map[string]bool
	embDirty map[string]bool
	chunks   map[string]*cachedFileChunks  // relPath → cached chunks
	embs     map[string]*cachedFileEmbeds  // relPath → cached embeddings
}

type wikiCacheManifest struct {
	Version int                                `json:"v"`
	Files   map[string]*wikiCacheManifestEntry `json:"files"`
}

type wikiCacheManifestEntry struct {
	Hash       string   `json:"h"`
	ChunkCount int      `json:"n"`
	Mtime      int64    `json:"mt,omitempty"`   // file mtime UnixNano
	Size       int64    `json:"sz,omitempty"`   // file size in bytes
	Slug       string   `json:"slug,omitempty"` // wiki slug for this source file
	OutRefs    []string `json:"out_refs,omitempty"` // outgoing cross-ref titles (union of all chunks)
}

// cachedFileChunks stores the processed chunks for a single source file.
type cachedFileChunks struct {
	Version int           `json:"v"`
	Hash    string        `json:"h"`
	Chunks  []CachedChunk `json:"chunks"`
}

// cachedFileEmbeds stores the embedding vectors for chunks of a single source file.
type cachedFileEmbeds struct {
	Version int               `json:"v"`
	Hash    string            `json:"h"`
	Vectors map[string]string `json:"vectors"` // content_hash → base64-encoded float32 blob
}

// CachedChunk is the cacheable representation of a wiki chunk.
// This is what gets serialized to JSON shards.
type CachedChunk struct {
	Title       string   `json:"title"`
	Body        string   `json:"body"`
	Summary     string   `json:"summary,omitempty"`
	DocType     string   `json:"doc_type,omitempty"`
	Breadcrumb  string   `json:"breadcrumb,omitempty"`
	ParentTitle string   `json:"parent_title,omitempty"`
	ContentHash string   `json:"content_hash"`
	CrossRefs   []string `json:"cross_refs,omitempty"`
	IsMarkdown  bool     `json:"is_markdown,omitempty"`
}

// NewWikiProcessCache creates or loads a wiki process cache from the given directory.
func NewWikiProcessCache(cacheDir string) (*WikiProcessCache, error) {
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return nil, fmt.Errorf("wiki process cache dir: %w", err)
	}

	wc := &WikiProcessCache{
		dir:      cacheDir,
		dirty:    make(map[string]bool),
		embDirty: make(map[string]bool),
		chunks:   make(map[string]*cachedFileChunks),
		embs:     make(map[string]*cachedFileEmbeds),
		manifest: &wikiCacheManifest{
			Version: wikiProcessCacheVersion,
			Files:   make(map[string]*wikiCacheManifestEntry),
		},
	}

	mPath := filepath.Join(cacheDir, "manifest.json")
	raw, err := os.ReadFile(mPath)
	if err == nil && len(raw) > 0 {
		var loaded wikiCacheManifest
		if json.Unmarshal(raw, &loaded) == nil && loaded.Version == wikiProcessCacheVersion {
			wc.manifest = &loaded
			if wc.manifest.Files == nil {
				wc.manifest.Files = make(map[string]*wikiCacheManifestEntry)
			}
		}
	}

	return wc, nil
}

// HasChanged returns true if the file at relPath has changed (different hash)
// or is not in the cache.
func (wc *WikiProcessCache) HasChanged(relPath, contentHash string) bool {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	e, ok := wc.manifest.Files[relPath]
	if !ok {
		return true
	}
	return e.Hash != contentHash
}

// StatMatch returns (cachedHash, true) if the file's mtime and size match the
// cached stat, meaning the content is almost certainly unchanged. This avoids
// a ReadFile call entirely — same technique as git's index stat caching.
// Returns ("", false) if the stat doesn't match or the file isn't cached.
func (wc *WikiProcessCache) StatMatch(relPath string, mtime, size int64) (string, bool) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	e, ok := wc.manifest.Files[relPath]
	if !ok || e.Mtime == 0 || e.Size == 0 {
		return "", false
	}
	if e.Mtime == mtime && e.Size == size {
		return e.Hash, true
	}
	return "", false
}

// StoreMtime records the mtime+size into an existing manifest entry so
// future calls to StatMatch can skip ReadFile entirely.
func (wc *WikiProcessCache) StoreMtime(relPath string, mtime, size int64) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.manifest.Files[relPath]; ok {
		e.Mtime = mtime
		e.Size = size
		wc.dirty[""] = true // manifest changed — ensure Save() writes it to disk
	}
}

// StoreSlug records the wiki slug for a source file so AllStatEntries() can
// return it for FastPathCheck without needing slug recomputation.
func (wc *WikiProcessCache) StoreSlug(relPath, slug string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.manifest.Files[relPath]; ok && e.Slug != slug {
		e.Slug = slug
		wc.dirty[""] = true
	}
}

// GetOutRefs returns the stored outgoing cross-reference titles for a cache key.
// Returns nil if no refs were recorded (e.g. first run or unchanged-but-unchecked file).
func (wc *WikiProcessCache) GetOutRefs(cacheKey string) []string {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.manifest.Files[cacheKey]; ok {
		return e.OutRefs
	}
	return nil
}

// StoreOutRefs records the union of outgoing cross-reference titles for a
// source file. Called after processing a changed file so the next run can
// compare old vs new refs without loading shard files.
func (wc *WikiProcessCache) StoreOutRefs(cacheKey string, refs []string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.manifest.Files[cacheKey]; ok {
		e.OutRefs = refs
		wc.dirty[""] = true
	}
}

// AllCacheKeys returns every cache key currently in the manifest.
// Used to detect deleted source files before Prune removes them.
func (wc *WikiProcessCache) AllCacheKeys() []string {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	keys := make([]string, 0, len(wc.manifest.Files))
	for k := range wc.manifest.Files {
		keys = append(keys, k)
	}
	return keys
}

// Get returns the cached chunks for a source file, or nil if not cached
// or hash doesn't match.
func (wc *WikiProcessCache) Get(relPath, contentHash string) []CachedChunk {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	e, ok := wc.manifest.Files[relPath]
	if !ok || e.Hash != contentHash {
		return nil
	}

	// Try in-memory first.
	if c, ok := wc.chunks[relPath]; ok && c.Hash == contentHash {
		return c.Chunks
	}

	// Load from disk.
	loaded, err := loadShard[cachedFileChunks](wc.chunkShardPath(relPath))
	if err != nil || loaded.Hash != contentHash || loaded.Version != wikiProcessCacheVersion {
		return nil
	}
	wc.chunks[relPath] = loaded
	return loaded.Chunks
}

// CachedStatEntry holds the stat metadata for a single cached source file.
// Used by the parallel stat pre-check in BuildKnowledgeWiki.
type CachedStatEntry struct {
	RelPath string
	Hash    string
	Slug    string
	Mtime   int64 // UnixNano
	Size    int64
}

// AllStatEntries returns the mtime/size/hash/slug for every file in the manifest.
// Returns nil if the cache is empty or has no mtime data.
func (wc *WikiProcessCache) AllStatEntries() []CachedStatEntry {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if len(wc.manifest.Files) == 0 {
		return nil
	}
	entries := make([]CachedStatEntry, 0, len(wc.manifest.Files))
	for relPath, e := range wc.manifest.Files {
		if e.Mtime == 0 || e.Size == 0 {
			return nil // incomplete stat data — fall through to full Walk
		}
		entries = append(entries, CachedStatEntry{
			RelPath: relPath,
			Hash:    e.Hash,
			Slug:    e.Slug,
			Mtime:   e.Mtime,
			Size:    e.Size,
		})
	}
	return entries
}

// Store saves the processed chunks for a source file.
func (wc *WikiProcessCache) Store(relPath, contentHash string, chunks []CachedChunk) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	entry := &cachedFileChunks{
		Version: wikiProcessCacheVersion,
		Hash:    contentHash,
		Chunks:  chunks,
	}

	wc.manifest.Files[relPath] = &wikiCacheManifestEntry{
		Hash:       contentHash,
		ChunkCount: len(chunks),
	}
	wc.chunks[relPath] = entry
	wc.dirty[relPath] = true

	// When a file changes, its old embeddings are invalidated.
	// Remove the stale emb shard.
	if old, ok := wc.embs[relPath]; ok && old.Hash != contentHash {
		delete(wc.embs, relPath)
		_ = os.Remove(wc.embShardPath(relPath))
	}
}

// ---------------------------------------------------------------------------
// Embedding shards — per-file .emb.json
// ---------------------------------------------------------------------------

// GetEmbeddings returns cached embeddings for a source file as content_hash → raw blob.
// Returns nil if no embeddings are cached for this file.
func (wc *WikiProcessCache) GetEmbeddings(relPath string) map[string][]byte {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	e, ok := wc.manifest.Files[relPath]
	if !ok {
		return nil
	}

	// Try in-memory.
	if em, ok := wc.embs[relPath]; ok && em.Hash == e.Hash {
		return decodeEmbMap(em.Vectors)
	}

	// Load from disk.
	loaded, err := loadShard[cachedFileEmbeds](wc.embShardPath(relPath))
	if err != nil || loaded.Hash != e.Hash || loaded.Version != wikiProcessCacheVersion {
		return nil
	}
	wc.embs[relPath] = loaded
	return decodeEmbMap(loaded.Vectors)
}

// StoreEmbeddings saves embedding vectors for a source file's chunks.
// vectors maps content_hash → raw float32 blob.
func (wc *WikiProcessCache) StoreEmbeddings(relPath string, vectors map[string][]byte) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	e, ok := wc.manifest.Files[relPath]
	if !ok {
		return
	}

	encoded := make(map[string]string, len(vectors))
	for hash, blob := range vectors {
		encoded[hash] = base64.StdEncoding.EncodeToString(blob)
	}

	wc.embs[relPath] = &cachedFileEmbeds{
		Version: wikiProcessCacheVersion,
		Hash:    e.Hash,
		Vectors: encoded,
	}
	wc.embDirty[relPath] = true
}

// LoadAllEmbeddings aggregates embeddings from all per-file .emb.json shards
// into a single EmbeddingCache for use in Rebuild().
func (wc *WikiProcessCache) LoadAllEmbeddings() EmbeddingCache {
	wc.mu.Lock()
	paths := make([]string, 0, len(wc.manifest.Files))
	for p := range wc.manifest.Files {
		paths = append(paths, p)
	}
	wc.mu.Unlock()

	cache := make(EmbeddingCache)
	for _, relPath := range paths {
		embs := wc.GetEmbeddings(relPath)
		for hash, blob := range embs {
			cache[hash] = blob
		}
	}
	if len(cache) == 0 {
		return nil
	}
	return cache
}

// ExportAllEmbeddingsFromDB reads all embeddings from a WikiDB and stores them
// into per-file .emb.json shards. Called after embedding cycles to persist vectors.
func (wc *WikiProcessCache) ExportAllEmbeddingsFromDB(db *WikiDB) int {
	if db == nil {
		return 0
	}

	// Query all embeddings grouped by source file.
	db.mu.RLock()
	rows, err := db.db.Query(`
		SELECT c.source, c.content_hash, v.embedding
		FROM chunks_vec_map m
		JOIN chunks c ON c.id = m.chunk_id
		JOIN chunks_vec v ON v.rowid = m.vec_rowid
		WHERE c.content_hash != '' AND c.source != ''`)
	db.mu.RUnlock()
	if err != nil {
		return 0
	}
	defer func() { _ = rows.Close() }()

	// Group by source file.
	byFile := make(map[string]map[string][]byte) // source → {content_hash → blob}
	for rows.Next() {
		var source, hash string
		var blob []byte
		if err := rows.Scan(&source, &hash, &blob); err != nil {
			continue
		}
		if source == "" || hash == "" || len(blob) == 0 {
			continue
		}
		if byFile[source] == nil {
			byFile[source] = make(map[string][]byte)
		}
		byFile[source][hash] = blob
	}

	total := 0
	for relPath, vectors := range byFile {
		wc.StoreEmbeddings(relPath, vectors)
		total += len(vectors)
	}
	return total
}

// ---------------------------------------------------------------------------
// Remove / Prune / Save
// ---------------------------------------------------------------------------

// Remove deletes a source file from the cache (both chunks and embeddings).
func (wc *WikiProcessCache) Remove(relPath string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	delete(wc.manifest.Files, relPath)
	delete(wc.chunks, relPath)
	delete(wc.embs, relPath)
	delete(wc.dirty, relPath)
	delete(wc.embDirty, relPath)
	_ = os.Remove(wc.chunkShardPath(relPath))
	_ = os.Remove(wc.embShardPath(relPath))
	wc.dirty[""] = true // mark manifest dirty
}

// Prune removes cached entries for files that no longer exist in the given set.
func (wc *WikiProcessCache) Prune(validPaths map[string]bool) int {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	pruned := 0
	for relPath := range wc.manifest.Files {
		if !validPaths[relPath] {
			delete(wc.manifest.Files, relPath)
			delete(wc.chunks, relPath)
			delete(wc.embs, relPath)
			_ = os.Remove(wc.chunkShardPath(relPath))
			_ = os.Remove(wc.embShardPath(relPath))
			pruned++
		}
	}
	if pruned > 0 {
		wc.dirty[""] = true
	}
	return pruned
}

// Save writes all dirty shards (chunks + embeddings) and the manifest to disk.
func (wc *WikiProcessCache) Save() error {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	if len(wc.dirty) == 0 && len(wc.embDirty) == 0 {
		return nil
	}

	var firstErr error

	// Flush dirty chunk shards.
	for relPath := range wc.dirty {
		if relPath == "" {
			continue
		}
		c := wc.chunks[relPath]
		if c == nil {
			continue
		}
		if err := writeWikiShard(wc.chunkShardPath(relPath), c); err != nil {
			if firstErr == nil {
				firstErr = fmt.Errorf("write wiki shard %s: %w", relPath, err)
			}
		}
	}

	// Flush dirty embedding shards.
	for relPath := range wc.embDirty {
		em := wc.embs[relPath]
		if em == nil {
			continue
		}
		if err := writeWikiShard(wc.embShardPath(relPath), em); err != nil {
			if firstErr == nil {
				firstErr = fmt.Errorf("write emb shard %s: %w", relPath, err)
			}
		}
	}

	// Write manifest.
	if err := writeWikiShard(filepath.Join(wc.dir, "manifest.json"), wc.manifest); err != nil {
		if firstErr == nil {
			firstErr = fmt.Errorf("write wiki manifest: %w", err)
		}
	}

	// Evict in-memory data for flushed shards.
	for relPath := range wc.dirty {
		if relPath != "" {
			delete(wc.chunks, relPath)
		}
	}
	for relPath := range wc.embDirty {
		delete(wc.embs, relPath)
	}
	wc.dirty = make(map[string]bool)
	wc.embDirty = make(map[string]bool)
	return firstErr
}

// Close flushes and saves the cache.
func (wc *WikiProcessCache) Close() error { return wc.Save() }

// Count returns the number of cached files.
func (wc *WikiProcessCache) Count() int {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	return len(wc.manifest.Files)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

func (wc *WikiProcessCache) chunkShardPath(relPath string) string {
	return filepath.Join(wc.dir, "shards", relPath+".wiki.json")
}

func (wc *WikiProcessCache) embShardPath(relPath string) string {
	return filepath.Join(wc.dir, "shards", relPath+".emb.json")
}

func decodeEmbMap(encoded map[string]string) map[string][]byte {
	if len(encoded) == 0 {
		return nil
	}
	result := make(map[string][]byte, len(encoded))
	for hash, b64 := range encoded {
		blob, err := base64.StdEncoding.DecodeString(b64)
		if err != nil || len(blob) == 0 {
			continue
		}
		result[hash] = blob
	}
	if len(result) == 0 {
		return nil
	}
	return result
}

func writeWikiShard(path string, data any) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	raw, err := json.Marshal(data)
	if err != nil {
		return err
	}
	tmpPath := path + ".tmp"
	if err := os.WriteFile(tmpPath, raw, 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmpPath, path); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}
	return nil
}

func loadShard[T any](path string) (*T, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var data T
	if err := json.Unmarshal(raw, &data); err != nil {
		return nil, err
	}
	return &data, nil
}
