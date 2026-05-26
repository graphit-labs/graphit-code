package ast

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"sync"
)

type HashCache struct {
	mu     sync.Mutex
	hashes map[string]string
	path   string
	dirty  bool
}

func NewHashCache(cacheDir string) *HashCache {
	cachePath := filepath.Join(cacheDir, "file_hashes.json")
	hc := &HashCache{
		hashes: make(map[string]string),
		path:   cachePath,
	}

	data, err := os.ReadFile(cachePath)
	if err == nil {
		_ = json.Unmarshal(data, &hc.hashes)
	}
	return hc
}

func (hc *HashCache) FileChanged(path string) bool {
	content, err := os.ReadFile(path)
	if err != nil {
		return true
	}

	sum := sha256.Sum256(content)
	newHash := hex.EncodeToString(sum[:])

	hc.mu.Lock()
	defer hc.mu.Unlock()

	oldHash, exists := hc.hashes[path]
	if exists && oldHash == newHash {
		return false
	}

	hc.hashes[path] = newHash
	hc.dirty = true
	return true
}

func (hc *HashCache) Save() error {
	hc.mu.Lock()
	defer hc.mu.Unlock()

	if !hc.dirty {
		return nil
	}

	if err := os.MkdirAll(filepath.Dir(hc.path), 0o755); err != nil {
		return err
	}

	data, err := json.Marshal(hc.hashes)
	if err != nil {
		return err
	}
	return os.WriteFile(hc.path, data, 0o644)
}

func (hc *HashCache) Count() int {
	hc.mu.Lock()
	defer hc.mu.Unlock()
	return len(hc.hashes)
}

func (hc *HashCache) Invalidate() {
	hc.mu.Lock()
	defer hc.mu.Unlock()
	hc.hashes = make(map[string]string)
	hc.dirty = false
	_ = os.Remove(hc.path)
}
