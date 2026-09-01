package ast

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// The embedding cache exists so that a rebuild does not have to recompute what the model
// already produced. It is NOT where a query reads a vector from — that is the entity's row in
// the search index — and it is not a second opinion about which vector is current: a shard's
// entry is keyed on the file's content hash, so a changed file invalidates its own vectors.
//
// It is written as BINARY, and the reason is measured rather than stylistic. The same 39,762
// vectors of 768 float32 occupy 122 MB as raw little-endian float32 and 381 MB as the JSON
// decimal text this replaced — a 3.1x inflation from serialisation alone, on what was 55% of
// the whole store.
//
// Halving it again by storing float16 is available and deliberately not taken: the cache is
// what a rebuild restores INTO the search index, so a lossy cache would make a store's vectors
// differ before and after a rebuild. That is a change to search behaviour and would need a
// measurement, not an assumption.

const shardEmbVersion = 5

type ShardEmbCache struct {
	dir   string
	mu    sync.Mutex
	data  map[string]*shardEmb
	dirty map[string]bool
}

type shardEmb struct {
	Hash       string
	Embeddings map[string][]float32
}

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
		if !hasFileSuffix(path, shardEmbSuffix) {
			return nil
		}

		rel, relErr := filepath.Rel(shardsDir, path)
		if relErr != nil {
			return nil
		}
		relPath := rel[:len(rel)-len(shardEmbSuffix)]

		loaded, loadErr := readEmbShard(path)
		if loadErr != nil {
			// An unreadable cache is dropped rather than carried.
			// The vectors are recomputable, which is the whole reason this is a cache.
			_ = os.Remove(path)
			return nil
		}
		ec.data[relPath] = loaded
		return nil
	})

	if parseCache != nil {
		ec.prune(parseCache)
	}

	return ec, nil
}

const shardEmbSuffix = ".emb.zst"

func (ec *ShardEmbCache) prune(parseCache *ShardCache) {
	parseCache.mu.Lock()
	defer parseCache.mu.Unlock()

	for relPath, emb := range ec.data {
		me, exists := parseCache.manifest.Files[relPath]
		if !exists || me.Hash != emb.Hash {
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
		emb = &shardEmb{Hash: contentHash, Embeddings: make(map[string][]float32)}
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
		if err := writeEmbShard(ec.shardPath(relPath), emb); err != nil && firstErr == nil {
			firstErr = fmt.Errorf("write emb shard %s: %w", relPath, err)
		}
	}

	ec.dirty = make(map[string]bool)
	return firstErr
}

func (ec *ShardEmbCache) Close() error { return ec.Save() }

func (ec *ShardEmbCache) shardPath(relPath string) string {
	return filepath.Join(ec.dir, "shards", relPath+shardEmbSuffix)
}

// ---------- the on-disk format ----------
//
// One independent Zstandard frame containing:
// version u16 | dim u16 | hash len u16 | hash | count u32
// then count records of: uid len u16 | uid | dim x float32 little-endian.
//
// Fixed dimension in the header rather than per record: every vector this model produces has
// the same width, and storing it once is what makes a record exactly len(uid)+2+dim*4 bytes.

func writeEmbShard(path string, emb *shardEmb) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	var raw bytes.Buffer
	if err := encodeEmbShard(&raw, emb); err != nil {
		return err
	}
	compressed := shardZstdEncoder.EncodeAll(raw.Bytes(), nil)
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, compressed, 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}

func encodeEmbShard(w io.Writer, emb *shardEmb) error {
	var err error
	put := func(v any) {
		if err == nil {
			err = binary.Write(w, binary.LittleEndian, v)
		}
	}
	putBytes := func(b []byte) {
		if err == nil {
			_, err = w.Write(b)
		}
	}

	// The active dimension, not the local model's fixed constant: a shard written under one
	// ai.embedding.provider must record ITS width, so readEmbShard's check below correctly
	// treats a shard from a since-abandoned provider as stale rather than misreading its bytes.
	dim := ai.ResolveConfiguredEmbeddingDimensions()

	hash := []byte(emb.Hash)
	put(uint16(shardEmbVersion))
	put(uint16(dim))
	put(uint16(len(hash)))
	putBytes(hash)
	put(uint32(countWritable(emb.Embeddings, dim)))

	for uid, vec := range emb.Embeddings {
		// A vector of the wrong width would desynchronise every record after it, since the
		// reader takes the width from the header. countWritable excluded it from the count.
		if len(vec) != dim {
			continue
		}
		put(uint16(len(uid)))
		putBytes([]byte(uid))
		put(vec)
	}
	if err != nil {
		return err
	}
	return nil
}

func countWritable(vecs map[string][]float32, dim int) int {
	n := 0
	for _, v := range vecs {
		if len(v) == dim {
			n++
		}
	}
	return n
}

func readEmbShard(path string) (*shardEmb, error) {
	compressed, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	raw, err := shardZstdDecoder.DecodeAll(compressed, nil)
	if err != nil {
		return nil, fmt.Errorf("decompress emb shard %s: %w", path, err)
	}
	r := bytes.NewReader(raw)

	var version, dim, hashLen uint16
	if err := binary.Read(r, binary.LittleEndian, &version); err != nil {
		return nil, err
	}
	if version != shardEmbVersion {
		return nil, fmt.Errorf("emb shard %s: version %d, want %d", path, version, shardEmbVersion)
	}
	if err := binary.Read(r, binary.LittleEndian, &dim); err != nil {
		return nil, err
	}
	// A shard whose stored width does not match what ai.embedding.provider currently resolves
	// to is not corrupt — it is a leftover from a provider or model that is no longer active.
	// Rejecting it here is what makes NewShardEmbCache's load loop drop it as "an older
	// format" and let it be recomputed, which is the correct response to a provider switch:
	// nothing needs bespoke migration logic, this cache is defined to be fully recomputable.
	if want := ai.ResolveConfiguredEmbeddingDimensions(); int(dim) != want {
		return nil, fmt.Errorf("emb shard %s: dim %d, want %d (ai.embedding.provider changed since this shard was written)", path, dim, want)
	}
	if err := binary.Read(r, binary.LittleEndian, &hashLen); err != nil {
		return nil, err
	}
	hash := make([]byte, hashLen)
	if _, err := io.ReadFull(r, hash); err != nil {
		return nil, err
	}
	var count uint32
	if err := binary.Read(r, binary.LittleEndian, &count); err != nil {
		return nil, err
	}

	out := &shardEmb{
		Hash:       string(hash),
		Embeddings: make(map[string][]float32, count),
	}
	for i := uint32(0); i < count; i++ {
		var uidLen uint16
		if err := binary.Read(r, binary.LittleEndian, &uidLen); err != nil {
			return nil, err
		}
		uid := make([]byte, uidLen)
		if _, err := io.ReadFull(r, uid); err != nil {
			return nil, err
		}
		vec := make([]float32, dim)
		if err := binary.Read(r, binary.LittleEndian, vec); err != nil {
			return nil, err
		}
		out.Embeddings[string(uid)] = vec
	}
	return out, nil
}

func hasFileSuffix(path, suffix string) bool {
	return len(path) > len(suffix) && path[len(path)-len(suffix):] == suffix
}
