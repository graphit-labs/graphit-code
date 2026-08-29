package ast

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"os"
)

// engineOwnedRelTypes are the relation types the ENGINE routes through a path of its
// own, and which therefore must not be written as a generic relation edge.
var engineOwnedRelTypes = map[string]bool{
	RelCalls: true, "INSTANTIATES": true,
	RelReadsField: true, RelWritesField: true,
	RelInherits: true, RelImplements: true, RelImports: true,
	"DECORATOR": true, "EXPORT": true,
}

// shortHex is the unique suffix used to name temporary working directories beside a
// live store — a build target and its orphan cleanup, but never a store path itself.
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

// copyBatchBytes caps the payload of one COPY in paths that still batch rows into
// documents. It is measured by bytes, not rows, because row sizes span six orders
// of magnitude: an entity row is tens of bytes and a File row is its whole source.
const copyBatchBytes = 64 << 20

// batchRows splits rows into groups whose estimated JSON payload stays under
// maxBytes. A row larger than the budget still gets its own batch: the point is to
// bound the document, not to reject content.
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

// estimateRowBytes approximates a row's encoded size. Only strings are measured
// exactly — they are the only values that can be arbitrarily large — and everything
// else is charged a flat width plus per-key overhead for quotes and separators.
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

// writeJSONFile writes a JSON array of rows to a path.
func writeJSONFile(path string, data []map[string]any) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer func() { _ = f.Close() }()
	return json.NewEncoder(f).Encode(data)
}
