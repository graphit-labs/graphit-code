package ast

import (
	"os"
	"path/filepath"
	"testing"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

func manifestFilesExist(t *testing.T, dir string, man *ladybug.CanonicalManifest) []string {
	t.Helper()
	var missing []string
	check := func(f string) {
		if f == "" {
			return
		}
		if _, err := os.Stat(filepath.Join(dir, f)); err != nil {
			missing = append(missing, f)
		}
	}
	for _, nt := range man.NodeTables {
		check(nt.File)
	}
	for _, rg := range man.RelGroups {
		for _, m := range rg.Members {
			check(m.Indices)
			check(m.Indptr)
		}
		for _, m := range rg.ReverseMembers {
			check(m.Indices)
			check(m.Indptr)
		}
	}
	return missing
}

// An incremental export regenerates the affected tables into a scratch directory and copies the
// unaffected ones from the previous bundle. It has to PUBLISH the regenerated ones too.
//
// It did not. Every copy went from the old bundle into the output, nothing ever came out of
// scratch, and scratch was deleted at the end — so each incremental silently dropped exactly the
// tables it had just rebuilt, while writing a manifest that still named them. OBSERVED on a
// 38k-file corpus: 549 Parquet files -> 175 -> 57 over three runs.
func TestIncrementalExportPublishesTheTablesItRegenerated(t *testing.T) {
	store := t.TempDir()
	finalDir := filepath.Join(store, "graph.icebug")

	base := newRebuildIndex(map[string]*parseCacheEntry{
		"a.go": {RelPath: "a.go", Language: "go", Entities: []cachedEntity{
			{Label: "Function", UID: "a.go::Alpha", Name: "Alpha", Path: "a.go", Line: 1, EndLine: 2},
		}},
		"b.go": {RelPath: "b.go", Language: "go", Entities: []cachedEntity{
			{Label: "Function", UID: "b.go::Beta", Name: "Beta", Path: "b.go", Line: 1, EndLine: 2},
		}},
	}, nil)
	if _, err := ExportDirectFromRebuildIndex(base, finalDir, finalDir); err != nil {
		t.Fatalf("base export: %v", err)
	}

	next := newRebuildIndex(map[string]*parseCacheEntry{
		"a.go": {RelPath: "a.go", Language: "go", Entities: []cachedEntity{
			{Label: "Function", UID: "a.go::Alpha", Name: "Alpha", Path: "a.go", Line: 1, EndLine: 2},
		}},
		"b.go": {RelPath: "b.go", Language: "go", Entities: []cachedEntity{
			{Label: "Function", UID: "b.go::BetaRenamed", Name: "BetaRenamed", Path: "b.go", Line: 1, EndLine: 3},
		}},
	}, nil)

	outDir := filepath.Join(store, "graph.icebug.tmp.test")
	man, err := ExportDirectIncrementalWithReverse(next, outDir, finalDir, outDir, []string{"b.go"}, nil, true)
	if err != nil {
		t.Fatalf("incremental export: %v", err)
	}

	if missing := manifestFilesExist(t, outDir, man); len(missing) > 0 {
		t.Errorf("the manifest names %d file(s) the bundle does not contain: %v",
			len(missing), missing)
	}

	if len(man.NodeTables) == 0 {
		t.Fatal("the incremental produced no node tables at all")
	}
	entries, err := os.ReadDir(outDir)
	if err != nil {
		t.Fatal(err)
	}
	baseEntries, err := os.ReadDir(finalDir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) < len(baseEntries) {
		var b, o []string
		for _, e := range baseEntries {
			b = append(b, e.Name())
		}
		for _, e := range entries {
			o = append(o, e.Name())
		}
		t.Errorf("the incremental bundle has %d files, fewer than the %d it started from\nbase: %v\nout:  %v\ntables: %d rels: %d",
			len(entries), len(baseEntries), b, o, len(man.NodeTables), len(man.RelGroups))
	}
}
