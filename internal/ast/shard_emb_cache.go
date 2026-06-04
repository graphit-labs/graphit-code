package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

type ShardEmbCache struct {
	dir   string
	mu    sync.Mutex
	data  map[string]*shardEmb
	dirty map[string]bool
}

type shardEmb struct {
	Version    int                  `json:"v"`
	Hash       string               `json:"h"`
	Embeddings map[string][]float32 `json:"emb"`
}

const shardEmbVersion = 2

func NewShardEmbCache(cacheDir string, parseCache *ShardCache) (*ShardEmbCache, error) {
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return nil, fmt.Errorf("shard emb cache dir: %w", err)
	}

	ec := &ShardEmbCache{
		dir:   cacheDir,
		data:  make(map[string]*shardEmb),
		dirty: make(map[string]bool),
	}

	shardsDir := filepath.Join(cacheDir, "shards")
	_ = filepath.Walk(shardsDir, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return nil
		}
		const suffix = ".emb.json"
		if !hasFileSuffix(path, suffix) {
			return nil
		}

		rel, _ := filepath.Rel(shardsDir, path)
		relPath := rel[:len(rel)-len(suffix)]

		loaded, loadErr := loadShard[shardEmb](path)
		if loadErr != nil || loaded.Version != shardEmbVersion {
			_ = os.Remove(path)
			return nil
		}
		if loaded.Embeddings == nil {
			loaded.Embeddings = make(map[string][]float32)
		}
		ec.data[relPath] = loaded
		return nil
	})

	_ = os.Remove(filepath.Join(cacheDir, "embeddings.json"))
	_ = os.Remove(filepath.Join(cacheDir, "embeddings.json.tmp"))

	if parseCache != nil {
		ec.prune(parseCache)
	}

	return ec, nil
}

func (ec *ShardEmbCache) prune(parseCache *ShardCache) {
	parseCache.mu.Lock()
	defer parseCache.mu.Unlock()

	for relPath, emb := range ec.data {
		me, exists := parseCache.manifest.Files[relPath]
		if !exists {

			delete(ec.data, relPath)
			_ = os.Remove(ec.shardPath(relPath))
			continue
		}
		if me.Hash != emb.Hash {

			delete(ec.data, relPath)
			_ = os.Remove(ec.shardPath(relPath))
		}
	}
}

func (ec *ShardEmbCache) Get(relPath, uid, currentHash string) []float32 {
	ec.mu.Lock()
	defer ec.mu.Unlock()
	emb, ok := ec.data[relPath]
	if !ok || emb.Hash != currentHash || emb.Embeddings == nil {
		return nil
	}
	return emb.Embeddings[uid]
}



func (ec *ShardEmbCache) Set(relPath, uid, contentHash string, vec []float32) {
	ec.mu.Lock()
	defer ec.mu.Unlock()
	emb, ok := ec.data[relPath]
	if !ok {
		emb = &shardEmb{
			Version:    shardEmbVersion,
			Hash:       contentHash,
			Embeddings: make(map[string][]float32),
		}
		ec.data[relPath] = emb
	} else if emb.Hash != contentHash {

		emb.Hash = contentHash
		emb.Embeddings = make(map[string][]float32)
	}
	emb.Embeddings[uid] = vec
	ec.dirty[relPath] = true
}

func (ec *ShardEmbCache) Save() error {
	ec.mu.Lock()
	defer ec.mu.Unlock()
	if len(ec.dirty) == 0 {
		return nil
	}

	var firstErr error
	for relPath := range ec.dirty {
		emb := ec.data[relPath]
		if emb == nil {
			continue
		}
		if err := writeShard(ec.shardPath(relPath), emb); err != nil {
			if firstErr == nil {
				firstErr = fmt.Errorf("write emb shard %s: %w", relPath, err)
			}
		}
	}

	ec.dirty = make(map[string]bool)
	return firstErr
}

func (ec *ShardEmbCache) Close() error { return ec.Save() }



func (ec *ShardEmbCache) shardPath(relPath string) string {
	return filepath.Join(ec.dir, "shards", relPath+".emb.json")
}

func hasFileSuffix(path, suffix string) bool {
	return len(path) > len(suffix) && path[len(path)-len(suffix):] == suffix
}
