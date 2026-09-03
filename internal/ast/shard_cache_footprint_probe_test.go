package ast

import (
	"fmt"
	"os"
	"runtime"
	"sort"
	"strconv"
	"testing"
)

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

	const header = 16
	var occurrences, byteTotal int64
	unique := make(map[string]int64)
	perField := make(map[string]*[2]int64)
	perFieldUnique := make(map[string]map[string]bool)
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
			perFieldUnique[field] = make(map[string]bool)
		}
		perFieldUnique[field][s] = true
		f[0]++
		f[1] += int64(len(s))
	}

	localSum := make(map[string]int64)
	localSeen := make(map[string]map[string]bool)
	noteLocal := func(field, s string) {
		m := localSeen[field]
		if m == nil {
			m = make(map[string]bool)
			localSeen[field] = m
		}
		m[s] = true
	}
	flushLocal := func() {
		for field, m := range localSeen {
			localSum[field] += int64(len(m))
		}
		localSeen = make(map[string]map[string]bool)
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
			noteLocal("call.CallerUID", c.CallerUID)
			noteLocal("call.CalleeUID", c.CalleeUID)
		}
		for _, in := range entry.Inheritance {
			note("inh.ChildUID", in.ChildUID)
			note("inh.ParentUID", in.ParentUID)
			noteLocal("inh.ChildUID", in.ChildUID)
			noteLocal("inh.ParentUID", in.ParentUID)
		}
		for _, fa := range entry.FieldAccess {
			note("fa.SourceUID", fa.SourceUID)
			note("fa.FieldUID", fa.FieldUID)
			noteLocal("fa.SourceUID", fa.SourceUID)
			noteLocal("fa.FieldUID", fa.FieldUID)
		}
		for _, ce := range entry.ContainsEdges {
			note("contains.ParentUID", ce.ParentUID)
			note("contains.ChildUID", ce.ChildUID)
			noteLocal("contains.ParentUID", ce.ParentUID)
			noteLocal("contains.ChildUID", ce.ChildUID)
		}
		flushLocal()
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
		distinct := len(perFieldUnique[r.field])
		line := fmt.Sprintf("  %-22s %10d occurrences %8.1f MB   distinct=%d (%.0f%% would dedupe)", r.field, r.n, float64(r.b)/(1<<20), distinct, 100*(1-float64(distinct)/float64(r.n)))
		if ls, ok := localSum[r.field]; ok {
			crossFileGap := ls - int64(distinct)
			line += fmt.Sprintf("   local-only-distinct(summed per file)=%d, CROSS-FILE duplicates a LOCAL interner would miss=%d (%.0f%% of this field's occurrences)",
				ls, crossFileGap, 100*float64(crossFileGap)/float64(r.n))
		}
		t.Logf("%s", line)
	}
}
