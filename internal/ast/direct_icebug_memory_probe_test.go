package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"testing"
	"time"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// TestExportDirectPeakHeap measures what the icebug export costs in memory for a corpus
// of a given size. It exists because a 120k-file repository was killed by the OOM killer
// in this exact code path while the bundle it was producing was not large.
//
// GRAPHIT_EXPORT_MEM=1 go test -run TestExportDirectPeakHeap ./internal/ast/ -v
// GRAPHIT_EXPORT_MEM=1 GRAPHIT_EXPORT_FILES=20000 go test -run TestExportDirectPeakHeap ./internal/ast/ -v
func TestExportDirectPeakHeap(t *testing.T) {
	if os.Getenv("GRAPHIT_EXPORT_MEM") == "" {
		t.Skip("set GRAPHIT_EXPORT_MEM=1")
	}
	files := 5000
	if v := os.Getenv("GRAPHIT_EXPORT_FILES"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil {
			t.Fatalf("GRAPHIT_EXPORT_FILES: %v", err)
		}
		files = n
	}

	entries := syntheticCorpus(files)
	ri := newRebuildIndex(entries, targetRulesFor(t.TempDir()))

	runtime.GC()
	var before runtime.MemStats
	runtime.ReadMemStats(&before)

	// HeapSys is a high-water of memory obtained from the OS and moves with GC timing;
	// the live peak is what decides whether the process survives, so sample it.
	stop := make(chan struct{})
	peak := make(chan uint64, 1)
	go func() {
		var m runtime.MemStats
		var max uint64
		for {
			select {
			case <-stop:
				peak <- max
				return
			default:
			}
			runtime.ReadMemStats(&m)
			if m.HeapAlloc > max {
				max = m.HeapAlloc
			}
			time.Sleep(2 * time.Millisecond)
		}
	}()

	out := filepath.Join(t.TempDir(), "graph.icebug")
	man, err := ExportDirectFromRebuildIndex(ri, out, out)
	close(stop)
	livePeak := <-peak
	if err != nil {
		t.Fatalf("export: %v", err)
	}

	var after runtime.MemStats
	runtime.ReadMemStats(&after)

	var bundle int64
	dirEntries, _ := os.ReadDir(out)
	for _, e := range dirEntries {
		if info, err := e.Info(); err == nil {
			bundle += info.Size()
		}
	}

	const mb = 1 << 20
	t.Logf("files=%d nodes=%d edges=%d", files, countRows(man.NodeTables), man.EdgeCount)
	t.Logf("bundle on disk: %d MB (%d files)", bundle/mb, len(dirEntries))
	t.Logf("live heap before export: %d MB", before.HeapAlloc/mb)
	t.Logf("PEAK live heap during export: %d MB (delta %d MB)", livePeak/mb, (livePeak-before.HeapAlloc)/mb)
	t.Logf("heap obtained from the OS: %d MB", after.HeapSys/mb)
	t.Logf("total allocated over the export: %d MB", (after.TotalAlloc-before.TotalAlloc)/mb)
}

func countRows(tables []ladybug.CanonicalNodeTable) int64 {
	var n int64
	for _, t := range tables {
		n += t.Rows
	}
	return n
}

// syntheticCorpus is one Go-shaped file per entry: a handful of declarations, the calls
// between them, and the containment edges the file owns.
func syntheticCorpus(files int) map[string]*parseCacheEntry {
	const entitiesPerFile = 12
	entries := make(map[string]*parseCacheEntry, files)
	for f := 0; f < files; f++ {
		rel := fmt.Sprintf("internal/pkg%d/module%d/file%d.go", f%97, f%53, f)
		e := &parseCacheEntry{
			RelPath:  rel,
			Language: "go",
			Cluster:  "core",
			DirPaths: []string{filepath.Dir(rel), fmt.Sprintf("internal/pkg%d", f%97)},
		}
		for i := 0; i < entitiesPerFile; i++ {
			uid := fmt.Sprintf("%s:Symbol%d", rel, i)
			e.Entities = append(e.Entities, cachedEntity{
				Label: "Function", UID: uid, Name: fmt.Sprintf("Symbol%d_%d", f, i),
				Path: rel, Line: i * 9, EndLine: i*9 + 7,
				Docstring: "Handles one step of the pipeline and returns the normalised result.",
				Lang:      "go", Complexity: 3, Context: "pkg", ContextType: "package",
				IsExported: i%2 == 0,
			})
			e.ContainsEdges = append(e.ContainsEdges, cachedContainsEdge{
				ParentUID: rel, ChildUID: uid, ParentLabel: "File", ChildLabel: "Function",
			})
			e.Calls = append(e.Calls, cachedCall{
				CallerUID: uid, CalleeUID: fmt.Sprintf("Symbol%d_%d", (f+1)%files, i),
				SourceType: "Function", Line: i * 9, Path: rel, Lang: "go",
			})
		}
		entries[rel] = e
	}
	return entries
}
