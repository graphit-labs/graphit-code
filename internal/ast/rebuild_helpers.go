package ast

import (
	"crypto/rand"
	"encoding/hex"
)

// engineOwnedRelTypes are the relation types the ENGINE routes through a path of its
// own, and which therefore must not be written as a generic relation edge.
var engineOwnedRelTypes = map[string]bool{
	RelCalls: true, "INSTANTIATES": true,
	RelReadsField: true, RelWritesField: true,
	RelInherits: true, RelImplements: true, RelImports: true,
	"DECORATOR": true, "EXPORT": true,
}

func shortHex() string {
	b := make([]byte, 4)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)[:7]
}

// BuildEmbLookup is how a rebuild restores the vectors the model already produced.
//
// A rebuild DROPS the entity table, so without this every rebuild would re-run the embedding
// model over the whole corpus. The vector is still read from the entity's row at query time —
// this is the durable copy that survives the drop, nothing more.
//
// It returns nil when no hash is cached for the file, which the index treats as "no vector":
// the row is still stored and searchable by keyword, and a later embedding cycle fills it in.
func BuildEmbLookup(cache *ShardCache, embCache *ShardEmbCache) func(relPath, uid string) []float32 {
	if embCache == nil || cache == nil {
		return nil
	}
	return func(relPath, uid string) []float32 {
		hash := cache.GetHash(relPath)
		if hash == "" {
			return nil
		}
		return embCache.Get(relPath, uid, hash)
	}
}

const copyBatchBytes = 64 << 20

func batchRows(data []map[string]any, maxBytes int) [][]map[string]any {
	if len(data) == 0 {
		return nil
	}
	if maxBytes <= 0 {
		return [][]map[string]any{data}
	}
	var batches [][]map[string]any
	start, size := 0, 0
	for i, row := range data {
		rowSize := estimateRowBytes(row)
		if i > start && size+rowSize > maxBytes {
			batches = append(batches, data[start:i])
			start, size = i, 0
		}
		size += rowSize
	}
	return append(batches, data[start:])
}

func estimateRowBytes(row map[string]any) int {
	n := 2
	for k, v := range row {
		n += len(k) + 6
		switch t := v.(type) {
		case string:
			n += len(t)
		case []float32:
			n += len(t) * 12
		default:
			n += 8
		}
	}
	return n
}
