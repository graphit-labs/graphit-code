package wiki

import (
	"context"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io/fs"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
)

// wikiProcessCacheVersion is stamped into every shard and sidecar. A cache at a
// different version is ignored rather than migrated, which costs one recompute.
//
// Bumped to 2 when the shared manifest.json was split into per-file sidecars and
// CachedChunk grew the fields that make a shard a complete chunk.
const wikiProcessCacheVersion = 2

// WikiProcessCache stores the processed chunks and embedding vectors of each
// source file, keyed by content hash. When a source file has not changed, its
// chunks and embeddings are reused instead of being re-parsed and re-embedded.
// The cache is the source of truth — wiki.db is always rebuilt from it.
//
// Every file in the cache belongs to exactly one source file:
//
//	shards/<relPath>.wiki.json  — processed chunks, complete enough to rebuild a WikiChunk
//	shards/<relPath>.emb.json   — embedding vectors (content_hash → base64 blob)
//	shards/<relPath>.meta.json  — hash, stat, slug and outgoing cross-refs
//	watch/<name>.json           — stat of a non-source file whose change invalidates the wiki
//
// # Why there is no shared index file
//
// This used to keep one manifest.json holding an entry per source file, which is
// the obvious design and the wrong one here: a memory wiki's cache travels in a
// git branch that EVERY developer on a team pushes to. Two people compiling
// independently produce two divergent versions of that single file, and git
// cannot merge JSON — so every concurrent push conflicts on it, and the rebase
// that the memory store relies on fails.
//
// Per-file sidecars remove the shared write target entirely. Two people adding
// different memories add different files, which git merges without being asked.
// The index is rebuilt by walking the shard tree on open, which costs one pass
// over a few hundred small files.
type WikiProcessCache struct {
	dir string

	mu       sync.Mutex
	files    map[string]*fileMeta         // relPath → metadata
	chunks   map[string]*cachedFileChunks // relPath → cached chunks
	embs     map[string]*cachedFileEmbeds // relPath → cached embeddings
	dirty    map[string]bool              // relPath → chunk shard needs writing
	embDirty map[string]bool              // relPath → emb shard needs writing
	metaDirt map[string]bool              // relPath → meta sidecar needs writing
}

// fileMeta is everything the cache knows about one source file besides its
// chunks and vectors. One sidecar per source file.
type fileMeta struct {
	Version    int      `json:"v"`
	Hash       string   `json:"h"`
	ChunkCount int      `json:"n"`
	Mtime      int64    `json:"mt,omitempty"`       // file mtime UnixNano
	Size       int64    `json:"sz,omitempty"`       // file size in bytes
	Slug       string   `json:"slug,omitempty"`     // wiki slug this source file produced
	OutRefs    []string `json:"out_refs,omitempty"` // outgoing cross-ref titles, union of all chunks
}

// watchFileEntry is the stat of a file that is not a source document but whose
// change still invalidates the wiki — an ignore file, for instance.
type watchFileEntry struct {
	Version int   `json:"v"`
	Mtime   int64 `json:"mt"`
	Size    int64 `json:"sz"`
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

// CachedChunk is the cacheable representation of a wiki chunk, and it is
// deliberately COMPLETE: every field of a WikiChunk that cannot be derived from
// the cache key or from the body is stored here.
//
// That completeness is what lets a consumer build wiki.db from the shards alone,
// with the source documents nowhere on the machine — which is how a published
// knowledge wiki is installed. Anything left out of this struct is silently lost
// on such an install, so a field that reaches the database belongs here.
//
// Derived rather than stored: Source is the cache key, Slug is in the sidecar
// (one per file, not one per chunk), and WordCount is counted from Body.
type CachedChunk struct {
	Title       string   `json:"title"`
	Body        string   `json:"body"`
	Summary     string   `json:"summary,omitempty"`
	DocType     string   `json:"doc_type,omitempty"`
	Breadcrumb  string   `json:"breadcrumb,omitempty"`
	ContentHash string   `json:"content_hash"`
	CrossRefs   []string `json:"cross_refs,omitempty"`
	IsMarkdown  bool     `json:"is_markdown,omitempty"`
	ClusterID   int      `json:"cluster_id,omitempty"`
	ClusterName string   `json:"cluster_name,omitempty"`
	Confidence  float64  `json:"confidence,omitempty"`
	Updated     string   `json:"updated,omitempty"`
	Important   bool     `json:"important,omitempty"`
}

const (
	chunkShardSuffix = ".wiki.json"
	embShardSuffix   = ".emb.json"
	metaShardSuffix  = ".meta.json"
	shardsDirName    = "shards"
	watchDirName     = "watch"
)

// NewWikiProcessCache creates or loads a wiki process cache from the given directory.
func NewWikiProcessCache(cacheDir string) (*WikiProcessCache, error) {
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return nil, fmt.Errorf("wiki process cache dir: %w", err)
	}

	wc := &WikiProcessCache{
		dir:      cacheDir,
		files:    make(map[string]*fileMeta),
		chunks:   make(map[string]*cachedFileChunks),
		embs:     make(map[string]*cachedFileEmbeds),
		dirty:    make(map[string]bool),
		embDirty: make(map[string]bool),
		metaDirt: make(map[string]bool),
	}
	wc.loadMeta()

	// A manifest.json left by version 1 is dead weight that would otherwise be
	// committed into a knowledge or memory branch and read by nobody.
	_ = os.Remove(filepath.Join(cacheDir, "manifest.json"))

	return wc, nil
}

// loadMeta rebuilds the in-memory index by walking the shard tree.
func (wc *WikiProcessCache) loadMeta() {
	shardsRoot := filepath.Join(wc.dir, shardsDirName)
	_ = filepath.WalkDir(shardsRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, metaShardSuffix) {
			return nil //nolint:nilerr // an unreadable entry is a cache miss, not a failure
		}
		rel, relErr := filepath.Rel(shardsRoot, path)
		if relErr != nil {
			return nil
		}
		meta, loadErr := loadShard[fileMeta](path)
		if loadErr != nil || meta.Version != wikiProcessCacheVersion || meta.Hash == "" {
			return nil
		}
		relPath := strings.TrimSuffix(rel, metaShardSuffix)
		wc.files[relPath] = meta
		return nil
	})
}

// HasChanged reports whether the file at relPath has changed, or is not cached.
func (wc *WikiProcessCache) HasChanged(relPath, contentHash string) bool {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	e, ok := wc.files[relPath]
	if !ok {
		return true
	}
	return e.Hash != contentHash
}

// StatMatch returns (cachedHash, true) when the file's mtime and size match the
// cached stat, meaning the content is almost certainly unchanged. This avoids a
// ReadFile entirely — the same technique as git's index stat caching.
func (wc *WikiProcessCache) StatMatch(relPath string, mtime, size int64) (string, bool) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	e, ok := wc.files[relPath]
	if !ok || e.Mtime == 0 || e.Size == 0 {
		return "", false
	}
	if e.Mtime == mtime && e.Size == size {
		return e.Hash, true
	}
	return "", false
}

// StoreMtime records mtime+size so a later StatMatch can skip ReadFile.
func (wc *WikiProcessCache) StoreMtime(relPath string, mtime, size int64) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.files[relPath]; ok && (e.Mtime != mtime || e.Size != size) {
		e.Mtime = mtime
		e.Size = size
		wc.metaDirt[relPath] = true
	}
}

// StoreSlug records the wiki slug a source file produced.
//
// It is what lets LoadAllChunks name a page without the source: a slug is
// assigned by the generator, may carry a disambiguating suffix when two titles
// collide, and is therefore not recomputable from one chunk in isolation.
func (wc *WikiProcessCache) StoreSlug(relPath, slug string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.files[relPath]; ok && e.Slug != slug {
		e.Slug = slug
		wc.metaDirt[relPath] = true
	}
}

// Slug returns the stored slug for a source file, or "" when none was recorded.
func (wc *WikiProcessCache) Slug(relPath string) string {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.files[relPath]; ok {
		return e.Slug
	}
	return ""
}

func (wc *WikiProcessCache) watchPath(name string) string {
	return filepath.Join(wc.dir, watchDirName, SafeSlug(name)+".json")
}

// StoreWatchFile records the stat of a non-source file that invalidates the wiki.
func (wc *WikiProcessCache) StoreWatchFile(name string, mtime, size int64) {
	_ = writeWikiShard(wc.watchPath(name), &watchFileEntry{
		Version: wikiProcessCacheVersion, Mtime: mtime, Size: size,
	})
}

// WatchFileChanged reports whether a watched file differs from its recorded stat.
// An unknown file counts as changed, so a first run never takes a fast path.
func (wc *WikiProcessCache) WatchFileChanged(name string, mtime, size int64) bool {
	e, err := loadShard[watchFileEntry](wc.watchPath(name))
	if err != nil || e.Version != wikiProcessCacheVersion {
		return true
	}
	return e.Mtime != mtime || e.Size != size
}

// GetOutRefs returns the recorded outgoing cross-reference titles for a cache key.
func (wc *WikiProcessCache) GetOutRefs(cacheKey string) []string {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.files[cacheKey]; ok {
		return e.OutRefs
	}
	return nil
}

// StoreOutRefs records the union of outgoing cross-reference titles for a source
// file, so the next run can compare old against new without loading shards.
func (wc *WikiProcessCache) StoreOutRefs(cacheKey string, refs []string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if e, ok := wc.files[cacheKey]; ok {
		e.OutRefs = refs
		wc.metaDirt[cacheKey] = true
	}
}

// AllCacheKeys returns every cache key currently known, so a caller can detect
// deleted source files before Prune removes them.
func (wc *WikiProcessCache) AllCacheKeys() []string {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	keys := make([]string, 0, len(wc.files))
	for k := range wc.files {
		keys = append(keys, k)
	}
	return keys
}

// Get returns the cached chunks for a source file, or nil when the file is not
// cached or its hash does not match.
func (wc *WikiProcessCache) Get(relPath, contentHash string) []CachedChunk {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	return wc.getLocked(relPath, contentHash)
}

func (wc *WikiProcessCache) getLocked(relPath, contentHash string) []CachedChunk {
	e, ok := wc.files[relPath]
	if !ok || e.Hash != contentHash {
		return nil
	}
	if c, ok := wc.chunks[relPath]; ok && c.Hash == contentHash {
		return c.Chunks
	}
	loaded, err := loadShard[cachedFileChunks](wc.chunkShardPath(relPath))
	if err != nil || loaded.Hash != contentHash || loaded.Version != wikiProcessCacheVersion {
		return nil
	}
	wc.chunks[relPath] = loaded
	return loaded.Chunks
}

// CachedStatEntry holds the stat metadata of one cached source file, for the
// parallel stat pre-check.
type CachedStatEntry struct {
	RelPath string
	Hash    string
	Mtime   int64 // UnixNano
	Size    int64
}

// AllStatEntries returns the stat of every cached file, or nil when the cache is
// empty or any entry lacks stat data — in which case the caller must walk.
func (wc *WikiProcessCache) AllStatEntries() []CachedStatEntry {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	if len(wc.files) == 0 {
		return nil
	}
	entries := make([]CachedStatEntry, 0, len(wc.files))
	for relPath, e := range wc.files {
		if e.Mtime == 0 || e.Size == 0 {
			return nil // incomplete stat data — fall through to a full walk
		}
		entries = append(entries, CachedStatEntry{
			RelPath: relPath, Hash: e.Hash, Mtime: e.Mtime, Size: e.Size,
		})
	}
	return entries
}

// Store saves the processed chunks for a source file.
//
// The file's other metadata survives: a slug, a stat and the outgoing refs are
// recorded by separate calls, and resetting them here would drop the slug of
// every file whose content changed.
func (wc *WikiProcessCache) Store(relPath, contentHash string, chunks []CachedChunk) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	meta, ok := wc.files[relPath]
	if !ok {
		meta = &fileMeta{Version: wikiProcessCacheVersion}
		wc.files[relPath] = meta
	}
	meta.Version = wikiProcessCacheVersion
	meta.Hash = contentHash
	meta.ChunkCount = len(chunks)

	wc.chunks[relPath] = &cachedFileChunks{
		Version: wikiProcessCacheVersion,
		Hash:    contentHash,
		Chunks:  chunks,
	}
	wc.dirty[relPath] = true
	wc.metaDirt[relPath] = true

	// A changed file invalidates its embeddings.
	if old, ok := wc.embs[relPath]; ok && old.Hash != contentHash {
		delete(wc.embs, relPath)
		_ = os.Remove(wc.embShardPath(relPath))
	}
}

// DerivedChunkFields are the parts of a chunk that are only known once the whole
// corpus has been processed: the slug it was assigned, and the community it landed
// in. Everything else about a chunk is known while its own file is being read.
type DerivedChunkFields struct {
	Slug        string
	ClusterID   int
	ClusterName string
	Confidence  float64
	Updated     string
	Important   bool
}

// StoreDerived records the corpus-level fields of a source file's chunks.
//
// It writes nothing when every value already matches, which is what keeps the
// cache incremental: a run where no document changed leaves no shard dirty, even
// though this is called for every document.
//
// Only the knowledge wiki needs this. A memory wiki is always compiled from its
// raw markdown — the shards exist there to spare the embedding model, not to
// stand in for the source — so its chunks do not have to be self-sufficient.
func (wc *WikiProcessCache) StoreDerived(relPath string, d DerivedChunkFields) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	meta, ok := wc.files[relPath]
	if !ok {
		return
	}
	if meta.Slug != d.Slug {
		meta.Slug = d.Slug
		wc.metaDirt[relPath] = true
	}

	cached := wc.getLocked(relPath, meta.Hash)
	if len(cached) == 0 {
		return
	}
	changed := false
	for i := range cached {
		c := &cached[i]
		if c.ClusterID == d.ClusterID && c.ClusterName == d.ClusterName &&
			c.Confidence == d.Confidence && c.Updated == d.Updated && c.Important == d.Important {
			continue
		}
		c.ClusterID, c.ClusterName = d.ClusterID, d.ClusterName
		c.Confidence, c.Updated, c.Important = d.Confidence, d.Updated, d.Important
		changed = true
	}
	if !changed {
		return
	}
	wc.chunks[relPath] = &cachedFileChunks{
		Version: wikiProcessCacheVersion,
		Hash:    meta.Hash,
		Chunks:  cached,
	}
	wc.dirty[relPath] = true
}

// LoadAllChunks reconstructs every chunk in the cache as a WikiChunk, together
// with the cross-reference map, WITHOUT reading a single source document.
//
// This is what makes a published wiki installable: the producer ships the shards,
// and the consumer builds its own wiki.db from them. A file with no recorded slug
// is skipped rather than guessed — a chunk with no page is unreachable, and
// inventing a slug would make it collide with a real one.
func (wc *WikiProcessCache) LoadAllChunks() ([]WikiChunk, map[string][]string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	relPaths := make([]string, 0, len(wc.files))
	for relPath := range wc.files {
		relPaths = append(relPaths, relPath)
	}
	sort.Strings(relPaths)

	out := make([]WikiChunk, 0, len(relPaths))
	xrefs := make(map[string][]string)
	for _, relPath := range relPaths {
		meta := wc.files[relPath]
		if meta.Slug == "" {
			continue
		}
		for _, c := range wc.getLocked(relPath, meta.Hash) {
			out = append(out, WikiChunk{
				Slug:        meta.Slug,
				Title:       c.Title,
				Body:        c.Body,
				Summary:     c.Summary,
				DocType:     c.DocType,
				Source:      relPath,
				Breadcrumb:  c.Breadcrumb,
				ClusterID:   c.ClusterID,
				ClusterName: c.ClusterName,
				Confidence:  c.Confidence,
				ContentHash: c.ContentHash,
				WordCount:   len(strings.Fields(c.Body)),
				Updated:     c.Updated,
				Important:   c.Important,
			})
			if len(c.CrossRefs) > 0 {
				refs := make([]string, 0, len(c.CrossRefs))
				for _, ref := range c.CrossRefs {
					refs = append(refs, SafeSlug(ref))
				}
				xrefs[meta.Slug] = refs
			}
		}
	}
	if len(xrefs) == 0 {
		xrefs = nil
	}
	return out, xrefs
}

// Embedding shards — per-file .emb.json

// GetEmbeddings returns cached embeddings for a source file as content_hash → blob.
func (wc *WikiProcessCache) GetEmbeddings(relPath string) map[string][]byte {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	e, ok := wc.files[relPath]
	if !ok {
		return nil
	}
	if em, ok := wc.embs[relPath]; ok && em.Hash == e.Hash {
		return decodeEmbMap(em.Vectors)
	}
	loaded, err := loadShard[cachedFileEmbeds](wc.embShardPath(relPath))
	if err != nil || loaded.Hash != e.Hash || loaded.Version != wikiProcessCacheVersion {
		return nil
	}
	wc.embs[relPath] = loaded
	return decodeEmbMap(loaded.Vectors)
}

// StoreEmbeddings saves embedding vectors for a source file's chunks.
func (wc *WikiProcessCache) StoreEmbeddings(relPath string, vectors map[string][]byte) {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	e, ok := wc.files[relPath]
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

// LoadAllEmbeddings aggregates every per-file .emb.json shard into one
// EmbeddingCache for Rebuild().
func (wc *WikiProcessCache) LoadAllEmbeddings() EmbeddingCache {
	wc.mu.Lock()
	paths := make([]string, 0, len(wc.files))
	for p := range wc.files {
		paths = append(paths, p)
	}
	wc.mu.Unlock()

	cache := make(EmbeddingCache)
	for _, relPath := range paths {
		for hash, blob := range wc.GetEmbeddings(relPath) {
			// The shard format stays a packed little-endian float32 blob — it is a
			// cache file format, not a storage-engine detail, and changing it would
			// invalidate every shard on disk for no gain. The decode happens here,
			// at the boundary, so nothing downstream handles bytes.
			if v := decodeFloat32Blob(blob); len(v) > 0 {
				cache[hash] = v
			}
		}
	}
	if len(cache) == 0 {
		return nil
	}
	return cache
}

// ExportAllEmbeddingsFromDB reads every embedding out of a WikiDB and stores it
// into per-file .emb.json shards, so vectors survive a rebuild.
func (wc *WikiProcessCache) ExportAllEmbeddingsFromDB(ctx context.Context, db *WikiDB) int {
	if db == nil {
		return 0
	}

	stored, err := db.StoredEmbeddings(ctx)
	if err != nil {
		return 0
	}

	byFile := make(map[string]map[string][]byte) // source → {content_hash → blob}
	for _, e := range stored {
		if e.Source == "" || e.ContentHash == "" || len(e.Vector) == 0 {
			continue
		}
		if byFile[e.Source] == nil {
			byFile[e.Source] = make(map[string][]byte)
		}
		byFile[e.Source][e.ContentHash] = encodeFloat32Blob(e.Vector)
	}

	total := 0
	for relPath, vectors := range byFile {
		wc.StoreEmbeddings(relPath, vectors)
		total += len(vectors)
	}
	return total
}

// Remove / Prune / Save

// Remove deletes a source file from the cache — chunks, embeddings and metadata.
func (wc *WikiProcessCache) Remove(relPath string) {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	wc.removeLocked(relPath)
}

func (wc *WikiProcessCache) removeLocked(relPath string) {
	delete(wc.files, relPath)
	delete(wc.chunks, relPath)
	delete(wc.embs, relPath)
	delete(wc.dirty, relPath)
	delete(wc.embDirty, relPath)
	delete(wc.metaDirt, relPath)
	chunkPath := wc.chunkShardPath(relPath)
	_ = os.Remove(chunkPath)
	_ = os.Remove(wc.embShardPath(relPath))
	_ = os.Remove(wc.metaShardPath(relPath))
	removeEmptyParents(filepath.Dir(chunkPath), filepath.Join(wc.dir, shardsDirName))
}

// Prune removes cached entries for files that are no longer in the given set.
func (wc *WikiProcessCache) Prune(validPaths map[string]bool) int {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	stale := make([]string, 0)
	for relPath := range wc.files {
		if !validPaths[relPath] {
			stale = append(stale, relPath)
		}
	}
	for _, relPath := range stale {
		wc.removeLocked(relPath)
	}
	return len(stale)
}

// Save writes every dirty shard and sidecar. There is no shared index to write.
func (wc *WikiProcessCache) Save() error {
	wc.mu.Lock()
	defer wc.mu.Unlock()

	if len(wc.dirty) == 0 && len(wc.embDirty) == 0 && len(wc.metaDirt) == 0 {
		return nil
	}

	var firstErr error
	note := func(err error, format string, relPath string) {
		if err != nil && firstErr == nil {
			firstErr = fmt.Errorf(format, relPath, err)
		}
	}

	for relPath := range wc.dirty {
		if c := wc.chunks[relPath]; c != nil {
			note(writeWikiShard(wc.chunkShardPath(relPath), c), "write wiki shard %s: %w", relPath)
		}
	}
	for relPath := range wc.embDirty {
		if em := wc.embs[relPath]; em != nil {
			note(writeWikiShard(wc.embShardPath(relPath), em), "write emb shard %s: %w", relPath)
		}
	}
	for relPath := range wc.metaDirt {
		if m := wc.files[relPath]; m != nil {
			note(writeWikiShard(wc.metaShardPath(relPath), m), "write meta shard %s: %w", relPath)
		}
	}

	// Evict the in-memory copies that are now on disk. The metadata index stays:
	// it is small, and every fast path reads it.
	for relPath := range wc.dirty {
		delete(wc.chunks, relPath)
	}
	for relPath := range wc.embDirty {
		delete(wc.embs, relPath)
	}
	wc.dirty = make(map[string]bool)
	wc.embDirty = make(map[string]bool)
	wc.metaDirt = make(map[string]bool)
	return firstErr
}

// Close flushes and saves the cache.
func (wc *WikiProcessCache) Close() error { return wc.Save() }

// Count returns the number of cached files.
func (wc *WikiProcessCache) Count() int {
	wc.mu.Lock()
	defer wc.mu.Unlock()
	return len(wc.files)
}

// Internal helpers

func (wc *WikiProcessCache) chunkShardPath(relPath string) string {
	return filepath.Join(wc.dir, shardsDirName, relPath+chunkShardSuffix)
}

func (wc *WikiProcessCache) embShardPath(relPath string) string {
	return filepath.Join(wc.dir, shardsDirName, relPath+embShardSuffix)
}

func (wc *WikiProcessCache) metaShardPath(relPath string) string {
	return filepath.Join(wc.dir, shardsDirName, relPath+metaShardSuffix)
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
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
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

func removeEmptyParents(dir, stopAt string) {
	for dir != stopAt && len(dir) > len(stopAt) {
		if err := os.Remove(dir); err != nil {
			break
		}
		dir = filepath.Dir(dir)
	}
}

// encodeFloat32Blob and decodeFloat32Blob are the shard cache's on-disk vector format: a
// packed little-endian float32 array.
//
// It was sqlite-vec's wire format, and the cache inherited it. It stays because it is a
// FILE format now, owned here, and changing it would invalidate every .emb.json shard on
// disk to save nothing. Nothing outside this file handles the bytes.
func encodeFloat32Blob(vec []float32) []byte {
	blob := make([]byte, len(vec)*4)
	for i, f := range vec {
		binary.LittleEndian.PutUint32(blob[i*4:], math.Float32bits(f))
	}
	return blob
}

func decodeFloat32Blob(blob []byte) []float32 {
	n := len(blob) / 4
	vec := make([]float32, n)
	for i := 0; i < n; i++ {
		vec[i] = math.Float32frombits(binary.LittleEndian.Uint32(blob[i*4:]))
	}
	return vec
}
