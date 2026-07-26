package ast

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode/utf8"
)

// TestOracleSourceIndexUTF8 reproduces, cheaply, the failure the full end-to-end run hits:
//
//	CALL CREATE_FTS_INDEX('SearchFile', 'sf_source', ['source'])
//	  -> Runtime exception: Failed calling LOWER: Invalid UTF-8.
//
// Three explanations were tested and refuted: the corpus is not the problem (all 35358 files
// decode as valid UTF-8, checked byte by byte), C1/C0 control characters are accepted, and so
// are multi-byte documents up to 2 MiB — larger than the corpus's biggest file.
//
// So the next thing to check is what is actually STORED, rather than what was read. This
// skips parsing entirely — the failing column holds raw file contents — which turns a
// four-minute pipeline run into a file-read loop, and then inspects the rows the engine
// refuses to index.
func TestOracleSourceIndexUTF8(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}

	limit := 0
	if v := os.Getenv("GRAPHIT_E2E_MAX_FILES"); v != "" {
		_, _ = fmt.Sscanf(v, "%d", &limit)
	}

	dir := t.TempDir()
	cache, err := NewShardCache(filepath.Join(dir, "cache"))
	if err != nil {
		t.Fatalf("cache: %v", err)
	}

	var paths []string
	invalidOnDisk := 0
	_ = filepath.WalkDir(src, func(p string, d fs.DirEntry, e error) error {
		if e != nil || d.IsDir() || !strings.HasSuffix(strings.ToLower(p), ".sql") {
			return nil
		}
		if limit > 0 && len(paths) >= limit {
			return filepath.SkipAll
		}
		b, rerr := os.ReadFile(p)
		if rerr != nil {
			return nil
		}
		if !utf8.Valid(b) {
			invalidOnDisk++
		}
		rel, _ := filepath.Rel(src, p)
		if err := cache.Store(rel, "h-"+rel, &parseCacheEntry{
			RelPath: rel, Language: "sql", Source: string(b),
			Entities: []cachedEntity{{
				Label: "Procedure", UID: "u-" + rel, Name: strings.TrimSuffix(filepath.Base(rel), ".sql"),
				Path: rel, Line: 1, EndLine: 1,
			}},
		}); err != nil {
			t.Fatalf("store %s: %v", rel, err)
		}
		paths = append(paths, rel)
		return nil
	})
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	t.Logf("cached %d files (%d invalid UTF-8 on disk)", len(paths), invalidOnDisk)
	if len(paths) == 0 {
		t.Skip("no files")
	}

	si, err := OpenSearchIndex(filepath.Join(dir, "search"))
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { _ = si.Close() })

	rebuildErr := si.RebuildFromCache(cache, nil)
	t.Logf("RebuildFromCache -> %v", rebuildErr)

	updateErr := si.UpdateIncremental(cache, paths[:1], nil, nil)
	t.Logf("UpdateIncremental -> %v", updateErr)

	// Whatever the outcome, inspect what is stored: a value that left Go valid and comes
	// back invalid points at the write path, not the corpus.
	res, qerr := si.conn.Query(
		"MATCH (f:SearchFile) RETURN f.path AS p, f.source AS s")
	if qerr != nil {
		t.Fatalf("scan rows: %v", qerr)
	}
	defer res.Close()
	scanned, invalidStored := 0, 0
	var worst string
	for res.HasNext() {
		tup, e := res.Next()
		if e != nil {
			break
		}
		pv, _ := tup.GetValue(0)
		sv, _ := tup.GetValue(1)
		body, _ := sv.(string)
		scanned++
		if !utf8.ValidString(body) {
			invalidStored++
			if worst == "" {
				worst = fmt.Sprint(pv)
			}
			// Where the value breaks says what broke it: an offset on a power-of-two
			// boundary means a buffer truncation, an arbitrary one means something else.
			bad := -1
			for i := 0; i < len(body); {
				r, sz := utf8.DecodeRuneInString(body[i:])
				if r == utf8.RuneError && sz <= 1 {
					bad = i
					break
				}
				i += sz
			}
			onDisk := -1
			if b, rerr := os.ReadFile(filepath.Join(src, fmt.Sprint(pv))); rerr == nil {
				onDisk = len(b)
			}
			t.Logf("  invalid: %-38s stored=%d bytes, on disk=%d, first bad byte at %d (%.4f of stored)",
				fmt.Sprint(pv), len(body), onDisk, bad, float64(bad)/float64(len(body)+1))
		}
	}
	t.Logf("scanned %d stored rows: %d hold invalid UTF-8 (first: %q)", scanned, invalidStored, worst)

	if rebuildErr == nil && updateErr == nil {
		t.Skipf("neither path failed on %d files — raise GRAPHIT_E2E_MAX_FILES to reproduce", len(paths))
	}
	if invalidStored > 0 {
		t.Errorf("%d stored rows are invalid UTF-8 although every input was valid — the write path "+
			"corrupts the value, and the engine is right to refuse it", invalidStored)
	} else {
		t.Errorf("indexing failed with an invalid-UTF-8 error while every stored row is valid "+
			"UTF-8 by Go's definition — the engine's notion of validity is narrower; "+
			"rebuild=%v update=%v", rebuildErr, updateErr)
	}
}
