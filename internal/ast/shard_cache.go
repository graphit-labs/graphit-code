package ast

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"unicode/utf8"
)

// shardCacheVersion invalidates every cached shard when it changes: a manifest written
// under a different version is discarded, so every file looks changed and is parsed
// again.
//
// **Bump it whenever the shape of what gets cached changes** — a new entity kind, a
// different label, a field added to an entry. Entries are keyed by the file's content
// hash, so a change in the conversion logic does not move the key: without a bump, the
// new logic reaches only files that happen to be edited afterwards, and everyone else
// keeps the old graph while running the new binary.
//
// 2: imports became entities (Import / Include / Export) instead of edge records only.
// 3: captured names are trimmed, so a padded reference target resolves to its declaration.
// 4: the <script>/<style> body of a single-file component is parsed with its own
//
//	grammar, so a .vue or .svelte now yields imports, script entities and CALLS
//	where it previously yielded only markup.
//
// 5: entities, calls and references carry the language that PRODUCED them, not the
//
//	host file's, so an embedded block resolves against its own grammar's
//	declarations and TargetRules. Without the bump, a corpus whose embedded SQL
//	files are unchanged keeps resolving them as the host format.
//
// 6: an embedded block's statements are attributed to the HOST entity that contains
//
//	them instead of to the file, so the source of the edge changes for every file
//	that carries one; and an index now yields its table, its covered columns and a
//	UNIQUE marker where it used to yield a bare name; and the SQL family emits
//	column-grain writes whose target is qualified by its table.
//
// 7: JavaScript, TypeScript and TSX yield Pair/Value for a config or lookup object and
//
//	an Import for require(), so a file whose only content is `export default { … }`
//	stops being empty — measured on this repository, tailwind.config.js went from 0
//	entities to 83.
//
// 8: the host of an embedded block is the entity that CONTAINS it, not the one crossing
//
//	the line above it — so the source of every DML edge from an embedded block moves,
//	from the sibling element preceding the block to whatever unit the grammar declares
//	around it (or to the file, when it declares none). Without the bump, a corpus whose
//	files are unchanged keeps the previous, wrong source.
//
// 9: a keyword is no longer indexed as a call target (PL/SQL's `call_statement` makes a
//
//	bare identifier a complete call, and its non-reserved keyword list holds BEGIN,
//	DECLARE, IF and PROCEDURE), a trigger is no longer a possible call target at all, and
//	an embedded block may declare the wrapping a FRAGMENT needs to parse — which turns a
//	program unit body from nothing into its procedure and everything it calls.
//
// 10: the file's text left the nodes shard. A shard is a LOCAL artifact — it exists to
//
//	build the Parquet bundle and the Lance tables, and it never travels — so a copy of
//	text that is already in the working tree grows the store with the corpus and buys
//	nothing. A project reads the text from the tree; a store installed from elsewhere
//	reads it from its search index, which is what the text travels in.
//
// 11: the legacy file_row tuple left the nodes shard. It redundantly carried file metadata
//
//	and retained source text at index 5 when source indexing was enabled; cluster now has its
//	own field.
const shardCacheVersion = 11

type ShardCache struct {
	dir      string
	root     string
	manifest *shardManifest
	mu       sync.Mutex
	dirty    map[string]bool
	nodes    map[string]*shardNodes
	edges    map[string]*shardEdges
	// interner is corpus-wide and lives as long as the cache: the values it holds are
	// the ones repeated across files, so a table shared by every file is the point.
	interner *shardInterner
}

type shardManifest struct {
	Version int                            `json:"v"`
	Files   map[string]*shardManifestEntry `json:"files"`
}

type shardManifestEntry struct {
	Hash  string `json:"h"`
	Lang  string `json:"lang,omitempty"`
	Dep   bool   `json:"dep,omitempty"`
	Mtime int64  `json:"mtime,omitempty"` // UnixNano of last observed mtime
}

type shardNodes struct {
	Version  int               `json:"v"`
	Hash     string            `json:"h"`
	Lang     string            `json:"lang,omitempty"`
	Dep      bool              `json:"dep,omitempty"`
	Cluster  string            `json:"cluster,omitempty"`
	DirPaths []string          `json:"dir_paths,omitempty"`
	Entities []cachedEntity    `json:"entities,omitempty"`
	Params   []cachedParameter `json:"params,omitempty"`
	Fields   []cachedField     `json:"fields,omitempty"`
}

type shardEdges struct {
	Version     int                  `json:"v"`
	Hash        string               `json:"h"`
	Calls       []cachedCall         `json:"calls,omitempty"`
	Imports     []cachedImport       `json:"imports,omitempty"`
	Inheritance []cachedInheritance  `json:"inheritance,omitempty"`
	FieldAccess []cachedFieldAccess  `json:"field_access,omitempty"`
	References  []cachedReference    `json:"references,omitempty"`
	Contains    []cachedContainsEdge `json:"contains,omitempty"`
}

func NewShardCache(cacheDir string) (*ShardCache, error) {
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return nil, fmt.Errorf("shard cache dir: %w", err)
	}

	sc := &ShardCache{
		dir:      cacheDir,
		dirty:    make(map[string]bool),
		nodes:    make(map[string]*shardNodes),
		edges:    make(map[string]*shardEdges),
		interner: newShardInterner(shardInternLimit),
		manifest: &shardManifest{
			Version: shardCacheVersion,
			Files:   make(map[string]*shardManifestEntry),
		},
	}

	mPath := filepath.Join(cacheDir, "manifest.json")
	raw, err := os.ReadFile(mPath)
	usable := false
	if err == nil && len(raw) > 0 {
		var loaded shardManifest
		if json.Unmarshal(raw, &loaded) == nil && loaded.Version == shardCacheVersion {
			sc.manifest = &loaded
			if sc.manifest.Files == nil {
				sc.manifest.Files = make(map[string]*shardManifestEntry)
			}
			usable = true
		}
	}
	if !usable {
		// The manifest is gone or was written under another version, so every shard
		// beside it is about to be reparsed and none of them will be read again.
		// Deleting the directory is what actually returns their bytes: a shard whose
		// suffix no writer produces any more — `.emb.json`, once a second copy of every
		// vector — is reachable by nothing and would otherwise sit there forever.
		_ = os.RemoveAll(filepath.Join(cacheDir, "shards"))
	}

	return sc, nil
}

// SetRoot points the cache at the working tree its shards were parsed from, which is
// where SourceOf reads file text.
//
// A cache with no root resolves no source. That is not a degraded mode to work around:
// shards are a LOCAL artifact and never travel, so the only cache without a tree is one
// serving a store whose text lives in its search index, which answers for source
// directly. See SourceOf.
func (sc *ShardCache) SetRoot(root string) {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	sc.root = root
}

// SourceOf returns the file's text read from the working tree, and "" when there is no
// tree, or the file cannot be read, or it no longer hashes to what was parsed.
//
// A shard does not carry text and is not asked for it. Shards are a local artifact —
// they build the Parquet bundle and the Lance tables and never leave this machine — so
// the tree is the only place a text copy is worth keeping. The store that has no tree
// is one installed from elsewhere, and its text lives in its search index, which is
// what answers for source there (SearchIndex.FileSource / FileSourceAt).
//
// SAFETY: the manifest hash is the only thing establishing that the bytes on disk are
// still the bytes the shard describes. Returning text that fails that check would pair
// one file's entity offsets with another file's content.
func (sc *ShardCache) SourceOf(relPath string) string {
	sc.mu.Lock()
	root, me := sc.root, sc.manifest.Files[relPath]
	sc.mu.Unlock()
	if root == "" || me == nil || me.Hash == "" {
		return ""
	}
	data, err := os.ReadFile(filepath.Join(root, relPath))
	if err != nil || contentHashOf(data) != me.Hash {
		return ""
	}
	return toValidUTF8(string(data))
}

// toValidUTF8 replaces malformed byte sequences the way encoding/json does.
//
// SAFETY: this is not cosmetic. The text goes into an Arrow string column, and Arrow REJECTS
// a batch containing invalid UTF-8 — one bad byte fails the whole append, so a single file
// takes down the index write for every file batched with it.
//
// It used to be handled by accident: the text reached the index through a JSON shard, and
// Go's encoding/json substitutes U+FFFD on the way in. Reading the file directly removed the
// round trip and with it the substitution, which surfaced as
// `Invalid UTF8 sequence at string index 1` on a corpus of XML reports. Same substitution,
// now on purpose.
func toValidUTF8(s string) string {
	if utf8.ValidString(s) {
		return s
	}
	return strings.ToValidUTF8(s, string(utf8.RuneError))
}

func (sc *ShardCache) HasChanged(relPath, contentHash string) bool {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	e, ok := sc.manifest.Files[relPath]
	if !ok {
		return true
	}
	return e.Hash != contentHash
}

// NeedsHash returns true if the file at relPath needs SHA-256 hashing.
// If the cached mtime matches the observed mtime, we skip hashing entirely
// (the file is almost certainly unchanged). This is a fast pre-filter before
// the more expensive fileContentHash call.
func (sc *ShardCache) NeedsHash(relPath string, mtime int64) bool {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	e, ok := sc.manifest.Files[relPath]
	if !ok {
		return true // not in cache at all
	}
	if e.Mtime == 0 {
		return true // no mtime stored yet — need to hash and record it
	}
	return e.Mtime != mtime
}

// StoreMtime records the last-seen mtime for relPath in the manifest.
// Call this after a successful parse to prime the mtime cache for next sync.
func (sc *ShardCache) StoreMtime(relPath string, mtime int64) {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	if e, ok := sc.manifest.Files[relPath]; ok {
		e.Mtime = mtime
		sc.dirty[""] = true // manifest changed
	}
}

func (sc *ShardCache) AllPaths() []string {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	paths := make([]string, 0, len(sc.manifest.Files))
	for p := range sc.manifest.Files {
		paths = append(paths, p)
	}
	return paths
}

func (sc *ShardCache) Count() int {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	return len(sc.manifest.Files)
}

func (sc *ShardCache) GetHash(relPath string) string {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	e, ok := sc.manifest.Files[relPath]
	if !ok {
		return ""
	}
	return e.Hash
}

func (sc *ShardCache) GetEntry(relPath string) *parseCacheEntry {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	return sc.getEntryLocked(relPath)
}

func (sc *ShardCache) getEntryLocked(relPath string) *parseCacheEntry {
	me, ok := sc.manifest.Files[relPath]
	if !ok {
		return nil
	}

	n, e := sc.nodes[relPath], sc.edges[relPath]
	if n == nil || e == nil {
		var freshNodes *shardNodes
		var freshEdges *shardEdges
		var err error
		if n == nil {
			if freshNodes, err = loadShard[shardNodes](sc.shardPath(relPath, ".nodes.json")); err != nil {
				return nil
			}
		}
		if e == nil {
			if freshEdges, err = loadShard[shardEdges](sc.shardPath(relPath, ".edges.json")); err != nil {
				return nil
			}
		}
		sc.adoptShardsLocked(relPath, freshNodes, freshEdges)
		n, e = sc.nodes[relPath], sc.edges[relPath]
		if n == nil || e == nil {
			return nil
		}
	}

	return mergeShards(relPath, n, e, me.Lang, me.Dep)
}

// adoptShardsLocked compacts freshly decoded shards and puts them in the cache. Either may
// be nil, meaning that half of the file was already cached. Both halves share one local
// interner because an identifier a file declares in its nodes is the same identifier its
// edges point at.
func (sc *ShardCache) adoptShardsLocked(relPath string, n *shardNodes, e *shardEdges) {
	if n == nil && e == nil {
		return
	}
	local := newShardInterner(shardLocalInternLimit)
	if n != nil {
		n.compact(sc.interner, local)
		sc.nodes[relPath] = n
	}
	if e != nil {
		e.compact(sc.interner, local)
		sc.edges[relPath] = e
	}
}

func (sc *ShardCache) AllEntries() map[string]*parseCacheEntry {
	sc.mu.Lock()

	paths := make([]string, 0, len(sc.manifest.Files))
	var needLoad []string
	for p := range sc.manifest.Files {
		paths = append(paths, p)
		if sc.nodes[p] == nil || sc.edges[p] == nil {
			needLoad = append(needLoad, p)
		}
	}
	sc.mu.Unlock()

	if len(paths) == 0 {
		return nil
	}

	if len(needLoad) > 0 {
		type loadResult struct {
			path  string
			nodes *shardNodes
			edges *shardEdges
		}
		results := make([]loadResult, len(needLoad))
		var wg sync.WaitGroup
		sem := make(chan struct{}, SafeWorkers(0))

		for i, p := range needLoad {
			wg.Add(1)
			go func(idx int, relPath string) {
				defer wg.Done()
				sem <- struct{}{}
				defer func() { <-sem }()

				var n *shardNodes
				var e *shardEdges
				n, _ = loadShard[shardNodes](sc.shardPath(relPath, ".nodes.json"))
				e, _ = loadShard[shardEdges](sc.shardPath(relPath, ".edges.json"))
				results[idx] = loadResult{relPath, n, e}
			}(i, p)
		}
		wg.Wait()

		sc.mu.Lock()
		for _, r := range results {
			sc.adoptShardsLocked(r.path, r.nodes, r.edges)
		}
		sc.mu.Unlock()
	}

	sc.mu.Lock()
	defer sc.mu.Unlock()
	entries := make(map[string]*parseCacheEntry, len(paths))
	for _, p := range paths {
		entry := sc.getEntryLocked(p)
		if entry != nil {
			entries[p] = entry
		}
	}
	return entries
}

func (sc *ShardCache) Store(relPath, contentHash string, entry *parseCacheEntry) error {
	sc.mu.Lock()
	defer sc.mu.Unlock()

	n, e := splitEntry(entry, contentHash)

	sc.manifest.Files[relPath] = &shardManifestEntry{
		Hash: contentHash,
		Lang: entry.Language,
		Dep:  entry.IsDepend,
	}
	sc.nodes[relPath] = n
	sc.edges[relPath] = e
	sc.dirty[relPath] = true
	return nil
}

func (sc *ShardCache) Remove(relPath string) {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	if _, ok := sc.manifest.Files[relPath]; !ok {
		return
	}
	delete(sc.manifest.Files, relPath)
	delete(sc.nodes, relPath)
	delete(sc.edges, relPath)
	delete(sc.dirty, relPath)

	_ = os.Remove(sc.shardPath(relPath, ".nodes.json"))
	_ = os.Remove(sc.shardPath(relPath, ".edges.json"))
	_ = os.Remove(sc.shardPath(relPath, ".emb.json"))
	removeEmptyParents(filepath.Dir(sc.shardPath(relPath, "")), filepath.Join(sc.dir, "shards"))

	sc.dirty[""] = true
}

// StreamEntries iterates over all cached entries one at a time, loading each
// from disk and evicting it after the callback returns. This bounds memory to
// O(1) loaded shards instead of O(N). Return false from fn to stop early.
func (sc *ShardCache) StreamEntries(fn func(relPath string, entry *parseCacheEntry) bool) {
	sc.streamEntriesExcept(nil, fn)
}

// streamEntriesExcept is StreamEntries with a pre-load exclusion set. The skip
// check happens before GetEntry so a path being reparsed is not decoded only to
// be replaced moments later.
func (sc *ShardCache) streamEntriesExcept(skip map[string]bool, fn func(relPath string, entry *parseCacheEntry) bool) {
	sc.mu.Lock()
	paths := make([]string, 0, len(sc.manifest.Files))
	for p := range sc.manifest.Files {
		if skip[p] {
			continue
		}
		paths = append(paths, p)
	}
	sc.mu.Unlock()

	for _, p := range paths {
		entry := sc.GetEntry(p)
		if entry == nil {
			continue
		}
		keepGoing := fn(p, entry)

		sc.mu.Lock()
		delete(sc.nodes, p)
		delete(sc.edges, p)
		sc.mu.Unlock()

		if !keepGoing {
			break
		}
	}
}

// FlushDirty writes all dirty shards to disk and evicts them from the
// in-memory maps to bound memory usage during long indexing runs.
func (sc *ShardCache) FlushDirty() error {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	return sc.flushLocked(true)
}

func (sc *ShardCache) Save() error {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	return sc.flushLocked(true)
}

func (sc *ShardCache) flushLocked(evict bool) error {
	if len(sc.dirty) == 0 {
		return nil
	}

	var wg sync.WaitGroup
	errCh := make(chan error, len(sc.dirty)*2)
	sem := make(chan struct{}, SafeWorkers(0))

	var flushedPaths []string

	for relPath := range sc.dirty {
		if relPath == "" {
			continue
		}
		n := sc.nodes[relPath]
		e := sc.edges[relPath]
		if n == nil || e == nil {
			continue
		}

		flushedPaths = append(flushedPaths, relPath)

		wg.Add(2)
		go func(rp string, nd *shardNodes) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()
			if err := writeShard(sc.shardPath(rp, ".nodes.json"), nd); err != nil {
				errCh <- fmt.Errorf("write nodes %s: %w", rp, err)
			}
		}(relPath, n)

		go func(rp string, ed *shardEdges) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()
			if err := writeShard(sc.shardPath(rp, ".edges.json"), ed); err != nil {
				errCh <- fmt.Errorf("write edges %s: %w", rp, err)
			}
		}(relPath, e)
	}

	wg.Wait()
	close(errCh)

	var firstErr error
	for err := range errCh {
		if firstErr == nil {
			firstErr = err
		}
	}

	if err := writeShard(filepath.Join(sc.dir, "manifest.json"), sc.manifest); err != nil {
		if firstErr == nil {
			firstErr = fmt.Errorf("write manifest: %w", err)
		}
	}

	if evict {
		for _, rp := range flushedPaths {
			delete(sc.nodes, rp)
			delete(sc.edges, rp)
		}
	}

	sc.dirty = make(map[string]bool)
	return firstErr
}

func (sc *ShardCache) Close() error { return sc.Save() }

func (sc *ShardCache) Reload() {
	sc.mu.Lock()
	defer sc.mu.Unlock()

	raw, err := os.ReadFile(filepath.Join(sc.dir, "manifest.json"))
	if err != nil || len(raw) == 0 {
		return
	}
	var loaded shardManifest
	if json.Unmarshal(raw, &loaded) == nil && loaded.Version == shardCacheVersion {
		sc.manifest = &loaded
		if sc.manifest.Files == nil {
			sc.manifest.Files = make(map[string]*shardManifestEntry)
		}

		sc.nodes = make(map[string]*shardNodes)
		sc.edges = make(map[string]*shardEdges)
		sc.dirty = make(map[string]bool)
	}
}

func (sc *ShardCache) shardPath(relPath, suffix string) string {
	return filepath.Join(sc.dir, "shards", relPath+suffix)
}

func splitEntry(entry *parseCacheEntry, hash string) (*shardNodes, *shardEdges) {
	n := &shardNodes{
		Version:  shardCacheVersion,
		Hash:     hash,
		Lang:     entry.Language,
		Dep:      entry.IsDepend,
		Cluster:  entry.Cluster,
		DirPaths: entry.DirPaths,
		Entities: entry.Entities,
		Params:   entry.Parameters,
		Fields:   entry.Fields,
	}
	e := &shardEdges{
		Version:     shardCacheVersion,
		Hash:        hash,
		Calls:       entry.Calls,
		Imports:     entry.Imports,
		Inheritance: entry.Inheritance,
		FieldAccess: entry.FieldAccess,
		References:  entry.References,
		Contains:    entry.ContainsEdges,
	}
	return n, e
}

func mergeShards(relPath string, n *shardNodes, e *shardEdges, lang string, isDep bool) *parseCacheEntry {
	return &parseCacheEntry{
		RelPath:       relPath,
		Language:      lang,
		IsDepend:      isDep,
		Cluster:       n.Cluster,
		DirPaths:      n.DirPaths,
		Entities:      n.Entities,
		Parameters:    n.Params,
		Fields:        n.Fields,
		Calls:         e.Calls,
		Imports:       e.Imports,
		Inheritance:   e.Inheritance,
		FieldAccess:   e.FieldAccess,
		References:    e.References,
		ContainsEdges: e.Contains,
	}
}

func writeShard(path string, data any) error {
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

func removeEmptyParents(dir, stopAt string) {
	for dir != stopAt && len(dir) > len(stopAt) {
		if err := os.Remove(dir); err != nil {
			break
		}
		dir = filepath.Dir(dir)
	}
}
