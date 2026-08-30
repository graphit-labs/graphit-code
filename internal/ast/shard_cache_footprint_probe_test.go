package ast

import (
	"os"
	"runtime"
	"sort"
	"strconv"
	"testing"
)

// TestShardCacheFootprint measures what a REAL parse cache costs in live heap once the
// rebuild has retained it, in bytes per graph element, so the cost of a corpus can be
// projected from its shard count instead of guessed.
//
// It samples. Loading a whole large store is the thing that gets this process killed, and
// reproducing an OOM in order to study it is not a measurement, it is the same outage.
//
//	GRAPHIT_SHARD_FOOTPRINT=~/.graphit/ast/project/<id> \
//	GRAPHIT_SHARD_FILES=4000 \
//	go test -run TestShardCacheFootprint ./internal/ast/ -v -timeout 30m
func TestShardCacheFootprint(t *testing.T) {
	dir := os.Getenv("GRAPHIT_SHARD_FOOTPRINT")
	if dir == "" {
		t.Skip("set GRAPHIT_SHARD_FOOTPRINT=<ast store dir holding manifest.json and shards/>")
	}
	sample := 4000
	if v := os.Getenv("GRAPHIT_SHARD_FILES"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil {
			t.Fatalf("GRAPHIT_SHARD_FILES: %v", err)
		}
		sample = n
	}

	cache, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("open shard cache: %v", err)
	}
	total := cache.Count()
	if total == 0 {
		t.Fatalf("no files in the manifest at %s", dir)
	}

	paths := cache.AllPaths()
	sort.Strings(paths)
	if sample > len(paths) {
		sample = len(paths)
	}
	// Stride rather than take a prefix: shard sizes vary by orders of magnitude between
	// clusters, and the sorted prefix is one cluster.
	stride := len(paths) / sample
	if stride < 1 {
		stride = 1
	}
	picked := make([]string, 0, sample)
	for i := 0; i < len(paths) && len(picked) < sample; i += stride {
		picked = append(picked, paths[i])
	}

	runtime.GC()
	var before runtime.MemStats
	runtime.ReadMemStats(&before)

	entries := make(map[string]*parseCacheEntry, len(picked))
	var entities, edges int64
	for _, p := range picked {
		entry := cache.GetEntry(p)
		if entry == nil {
			continue
		}
		entries[p] = entry
		entities += int64(len(entry.Entities) + len(entry.Parameters) + len(entry.Fields))
		edges += int64(len(entry.Calls) + len(entry.Imports) + len(entry.Inheritance) +
			len(entry.FieldAccess) + len(entry.References) + len(entry.ContainsEdges))
	}

	runtime.GC()
	var held runtime.MemStats
	runtime.ReadMemStats(&held)
	retained := held.HeapAlloc - before.HeapAlloc

	ri := newRebuildIndex(entries, targetRulesFor(t.TempDir()))
	runtime.GC()
	var indexed runtime.MemStats
	runtime.ReadMemStats(&indexed)
	withIndex := indexed.HeapAlloc - before.HeapAlloc

	elements := entities + edges
	if elements == 0 {
		t.Fatalf("sampled %d files and found no graph elements", len(entries))
	}

	const mb = 1 << 20
	perElement := float64(retained) / float64(elements)
	perElementIndexed := float64(withIndex) / float64(elements)
	scale := float64(total) / float64(len(entries))

	t.Logf("store: %s", dir)
	t.Logf("files in manifest: %d, sampled: %d (stride %d)", total, len(entries), stride)
	t.Logf("sampled elements: %d entities + %d edges = %d", entities, edges, elements)
	t.Logf("RETAINED parse cache: %d MB -> %.0f B/element", retained/mb, perElement)
	t.Logf("plus the rebuild index: %d MB -> %.0f B/element", withIndex/mb, perElementIndexed)
	t.Logf("PROJECTED for the whole store (x%.1f): cache %.1f GB, cache+index %.1f GB",
		scale, float64(retained)*scale/(1<<30), float64(withIndex)*scale/(1<<30))

	runtime.KeepAlive(ri)
	runtime.KeepAlive(entries)
}

// TestShardCacheStringDuplication measures how much of a retained parse cache is the SAME
// string decoded again: `Path` is one value per file and the JSON decoder allocates it once
// per entity, and `Label`/`Lang`/`ContextType` have a cardinality in the dozens across a
// whole corpus. It reports what interning at decode time could return.
//
//	GRAPHIT_SHARD_FOOTPRINT=~/.graphit/ast/project/<id> \
//	go test -run TestShardCacheStringDuplication ./internal/ast/ -v
func TestShardCacheStringDuplication(t *testing.T) {
	dir := os.Getenv("GRAPHIT_SHARD_FOOTPRINT")
	if dir == "" {
		t.Skip("set GRAPHIT_SHARD_FOOTPRINT=<ast store dir holding manifest.json and shards/>")
	}
	sample := 2000
	if v := os.Getenv("GRAPHIT_SHARD_FILES"); v != "" {
		n, _ := strconv.Atoi(v)
		if n > 0 {
			sample = n
		}
	}

	cache, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("open shard cache: %v", err)
	}
	paths := cache.AllPaths()
	sort.Strings(paths)
	stride := len(paths) / sample
	if stride < 1 {
		stride = 1
	}

	// A string costs its bytes rounded up to Go's size class, plus a 16-byte header at
	// every use. Interning removes the bytes, never the header.
	const header = 16
	var occurrences, byteTotal int64
	unique := make(map[string]int64)
	perField := make(map[string]*[2]int64)
	note := func(field, s string) {
		occurrences++
		byteTotal += int64(len(s))
		if _, seen := unique[s]; !seen {
			unique[s] = int64(len(s))
		}
		f := perField[field]
		if f == nil {
			f = &[2]int64{}
			perField[field] = f
		}
		f[0]++
		f[1] += int64(len(s))
	}

	files := 0
	for i := 0; i < len(paths); i += stride {
		entry := cache.GetEntry(paths[i])
		if entry == nil {
			continue
		}
		files++
		for _, e := range entry.Entities {
			note("entity.Label", e.Label)
			note("entity.UID", e.UID)
			note("entity.Name", e.Name)
			note("entity.Path", e.Path)
			note("entity.Docstring", e.Docstring)
			note("entity.Lang", e.Lang)
			note("entity.Context", e.Context)
			note("entity.ContextType", e.ContextType)
			note("entity.Value", e.Value)
		}
		for _, c := range entry.Calls {
			note("call.CallerUID", c.CallerUID)
			note("call.CalleeUID", c.CalleeUID)
			note("call.SourceType", c.SourceType)
			note("call.Path", c.Path)
			note("call.ReceiverType", c.ReceiverType)
		}
	}

	var uniqueBytes int64
	for _, n := range unique {
		uniqueBytes += n
	}

	t.Logf("sampled %d files", files)
	t.Logf("string occurrences: %d, bytes if every one is its own allocation: %.1f MB",
		occurrences, float64(byteTotal)/(1<<20))
	t.Logf("distinct values: %d, their bytes: %.1f MB", len(unique), float64(uniqueBytes)/(1<<20))
	t.Logf("recoverable by interning: %.1f MB of %.1f MB (%.0f%% of string BYTES; headers stay: %.1f MB)",
		float64(byteTotal-uniqueBytes)/(1<<20), float64(byteTotal)/(1<<20),
		100*float64(byteTotal-uniqueBytes)/float64(byteTotal), float64(occurrences*header)/(1<<20))

	type row struct {
		field string
		n, b  int64
	}
	rows := make([]row, 0, len(perField))
	for f, v := range perField {
		rows = append(rows, row{f, v[0], v[1]})
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].b > rows[j].b })
	for _, r := range rows {
		t.Logf("  %-22s %10d occurrences %8.1f MB", r.field, r.n, float64(r.b)/(1<<20))
	}
}
