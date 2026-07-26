package ast

import (
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// TestOracleExtractionCensus reports, per Oracle object type, how much the parser actually
// extracts from the real corpus.
//
// It exists because the end-to-end benchmark reported "empty=799 errors=0" and that number
// was taken at face value. The cause is not the pipeline: parsing, the work directory
// layout, .astignore and file count were all varied and none of them mattered. What matters
// is WHICH files, and the corpus is organised one directory per object type — so the
// bounded end-to-end runs (GRAPHIT_E2E_MAX_FILES) only ever saw the first directory in walk
// order, mviews, and measured a full pipeline over an extraction that yields nothing.
//
// A per-type census is the measurement that should have existed first: it separates "the
// corpus does not parse" from "one object type does not extract", and it turns the
// end-to-end timings into something interpretable.
func TestOracleExtractionCensus(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}

	const perDir = 12

	byDir := map[string][]string{}
	_ = filepath.WalkDir(src, func(p string, d fs.DirEntry, e error) error {
		if e != nil || d.IsDir() || !strings.HasSuffix(strings.ToLower(p), ".sql") {
			return nil
		}
		rel, err := filepath.Rel(src, p)
		if err != nil {
			return nil
		}
		top := strings.SplitN(rel, string(os.PathSeparator), 2)[0]
		if len(byDir[top]) < perDir {
			byDir[top] = append(byDir[top], p)
		}
		return nil
	})
	if len(byDir) == 0 {
		t.Skip("no .sql files under GRAPHIT_E2E_SQL_DIR")
	}

	dirs := make([]string, 0, len(byDir))
	for d := range byDir {
		dirs = append(dirs, d)
	}
	sort.Strings(dirs)

	opts := ParseOptions{IndexSource: true}

	t.Logf("%-18s | %5s | %8s | %9s | %s", "object type", "files", "entities", "empty", "median/file")
	t.Logf("%s", strings.Repeat("-", 76))

	var totalFiles, totalEntities, totalEmpty int
	var dead []string
	for _, d := range dirs {
		files := byDir[d]
		entities, empty := 0, 0
		counts := make([]int, 0, len(files))
		for _, p := range files {
			comp := NewCompositeParser(filepath.Dir(p), nil)
			pf, err := comp.Parse(p, false, opts)
			n := 0
			if err == nil && pf != nil {
				n = pf.EntityCount()
			}
			counts = append(counts, n)
			entities += n
			if n == 0 {
				empty++
			}
		}
		sort.Ints(counts)
		median := 0
		if len(counts) > 0 {
			median = counts[len(counts)/2]
		}
		t.Logf("%-18s | %5d | %8d | %4d/%-4d | %d", d, len(files), entities, empty, len(files), median)

		totalFiles += len(files)
		totalEntities += entities
		totalEmpty += empty
		if entities == 0 {
			dead = append(dead, d)
		}
	}

	t.Logf("%s", strings.Repeat("-", 76))
	t.Logf("sampled %d files across %d object types: %d entities, %d empty (%.0f%%)",
		totalFiles, len(dirs), totalEntities, totalEmpty,
		100*float64(totalEmpty)/float64(totalFiles))
	if len(dead) > 0 {
		t.Logf("object types yielding NOTHING: %s", strings.Join(dead, ", "))
	}

	if totalEntities == 0 {
		t.Fatalf("no object type yielded a single entity — extraction is broken corpus-wide, not per type")
	}

	// The census is the deliverable, not a threshold: a type that legitimately holds no
	// named object reports zero, and "comments" is exactly that — COMMENT ON statements
	// name nothing to extract. What is asserted above is only that extraction is not dead
	// corpus-wide.
	//
	// The consequence for measurement is handled where it belongs: TestE2EIndex now samples
	// round-robin across these directories instead of taking a walk-order prefix, because
	// the prefix is "comments" and bounded runs were timing an empty extraction.
	if len(dead) > 0 && dead[0] == dirs[0] {
		t.Logf("NOTE: %q is both empty and first in walk order — the reason TestE2EIndex samples "+
			"across groups rather than taking a prefix", dirs[0])
	}
}
