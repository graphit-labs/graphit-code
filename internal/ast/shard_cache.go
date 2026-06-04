package ast

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

const shardCacheVersion = 1

type ShardCache struct {
	dir      string
	manifest *shardManifest
	mu       sync.Mutex
	dirty    map[string]bool
	nodes    map[string]*shardNodes
	edges    map[string]*shardEdges
}

type shardManifest struct {
	Version int                            `json:"v"`
	Files   map[string]*shardManifestEntry `json:"files"`
}

type shardManifestEntry struct {
	Hash string `json:"h"`
	Lang string `json:"lang,omitempty"`
	Dep  bool   `json:"dep,omitempty"`
}

type shardNodes struct {
	Version  int               `json:"v"`
	Hash     string            `json:"h"`
	Lang     string            `json:"lang,omitempty"`
	Dep      bool              `json:"dep,omitempty"`
	Source   string            `json:"src,omitempty"`
	FileRow  []string          `json:"file_row,omitempty"`
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
		dir:   cacheDir,
		dirty: make(map[string]bool),
		nodes: make(map[string]*shardNodes),
		edges: make(map[string]*shardEdges),
		manifest: &shardManifest{
			Version: shardCacheVersion,
			Files:   make(map[string]*shardManifestEntry),
		},
	}

	mPath := filepath.Join(cacheDir, "manifest.json")
	raw, err := os.ReadFile(mPath)
	if err == nil && len(raw) > 0 {
		var loaded shardManifest
		if json.Unmarshal(raw, &loaded) == nil && loaded.Version == shardCacheVersion {
			sc.manifest = &loaded
			if sc.manifest.Files == nil {
				sc.manifest.Files = make(map[string]*shardManifestEntry)
			}
		}

	}

	for _, legacy := range []string{
		filepath.Join(cacheDir, "cache.json"),
		filepath.Join(cacheDir, "cache.json.tmp"),
		filepath.Join(cacheDir, "embeddings.json"),
		filepath.Join(cacheDir, "embeddings.json.tmp"),
		filepath.Join(cacheDir, "parse.db"),
		filepath.Join(cacheDir, "parse.db-wal"),
		filepath.Join(cacheDir, "parse.db-shm"),
		filepath.Join(cacheDir, "hashes.json"),
	} {
		_ = os.Remove(legacy)
	}
	_ = os.RemoveAll(filepath.Join(cacheDir, "parsed"))

	return sc, nil
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

	n := sc.nodes[relPath]
	if n == nil {
		loaded, err := loadShard[shardNodes](sc.shardPath(relPath, ".nodes.json"))
		if err != nil {
			return nil
		}
		n = loaded
		sc.nodes[relPath] = n
	}

	e := sc.edges[relPath]
	if e == nil {
		loaded, err := loadShard[shardEdges](sc.shardPath(relPath, ".edges.json"))
		if err != nil {
			return nil
		}
		e = loaded
		sc.edges[relPath] = e
	}

	return mergeShards(relPath, n, e, me.Lang, me.Dep)
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
			if r.nodes != nil {
				sc.nodes[r.path] = r.nodes
			}
			if r.edges != nil {
				sc.edges[r.path] = r.edges
			}
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

	sc.dirty[""] = true
}




func (sc *ShardCache) Save() error {
	sc.mu.Lock()
	defer sc.mu.Unlock()

	if len(sc.dirty) == 0 {
		return nil
	}

	var wg sync.WaitGroup
	errCh := make(chan error, len(sc.dirty)*2)
	sem := make(chan struct{}, SafeWorkers(0))

	for relPath := range sc.dirty {
		if relPath == "" {
			continue
		}
		n := sc.nodes[relPath]
		e := sc.edges[relPath]
		if n == nil || e == nil {
			continue
		}

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
		Source:   entry.Source,
		FileRow:  entry.FileRow,
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
		Source:        n.Source,
		FileRow:       n.FileRow,
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
