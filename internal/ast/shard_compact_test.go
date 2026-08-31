package ast

import (
	"reflect"
	"testing"
	"unsafe"
)

// Compaction replaces string headers and slice backing arrays. The one thing it must never
// do is change a VALUE, so this compares a compacted round-trip against the plain decode of
// the same shards, field by field.
func TestCompactionPreservesEveryValue(t *testing.T) {
	dir := t.TempDir()
	cache, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("new cache: %v", err)
	}

	written := map[string]*parseCacheEntry{}
	for _, entry := range compactionCorpus() {
		written[entry.RelPath] = entry
		if err := cache.Store(entry.RelPath, "hash-"+entry.RelPath, entry); err != nil {
			t.Fatalf("store %s: %v", entry.RelPath, err)
		}
	}
	if err := cache.Save(); err != nil {
		t.Fatalf("save: %v", err)
	}

	reopened, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	for relPath, want := range written {
		got := reopened.GetEntry(relPath)
		if got == nil {
			t.Fatalf("%s: not found after reload", relPath)
		}
		// Source and legacy FileRow are not persisted in a shard, so they are fields a round-trip
		// legitimately drops.
		want.Source = ""
		want.FileRow = nil
		if !reflect.DeepEqual(got, want) {
			t.Errorf("%s: round-trip differs\n got: %+v\nwant: %+v", relPath, got, want)
		}
	}
}

// The point of interning is that a value occurring N times costs one allocation, so the
// test for it is pointer identity, not equality.
func TestCompactionSharesRepeatedStrings(t *testing.T) {
	dir := t.TempDir()
	cache, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("new cache: %v", err)
	}
	for _, entry := range compactionCorpus() {
		if err := cache.Store(entry.RelPath, "h", entry); err != nil {
			t.Fatalf("store: %v", err)
		}
	}
	if err := cache.Save(); err != nil {
		t.Fatalf("save: %v", err)
	}

	reopened, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	a := reopened.GetEntry("pkg/one.go")
	b := reopened.GetEntry("pkg/two.go")
	if a == nil || b == nil {
		t.Fatal("entries missing after reload")
	}

	// The same path repeats once per entity inside a shard and must cost one allocation.
	if data(a.Entities[0].Path) != data(a.Entities[1].Path) {
		t.Error("Path repeated inside a file is still two allocations")
	}
	if data(a.Entities[0].Lang) != data(b.Entities[0].Lang) {
		t.Error("Lang shared by two files is still two allocations")
	}
	if data(a.Entities[0].Label) != data(b.Entities[0].Label) {
		t.Error("Label shared by two files is still two allocations")
	}
	if data(a.Entities[0].Path) == data(b.Entities[0].Path) {
		t.Error("two DIFFERENT paths were folded into one value")
	}

	// A callee, a base class, and an accessed field are all "pointed at from many files";
	// interning them per-file (discarded per file) would miss the exact duplication that
	// happens when two DIFFERENT files reference the same one. These three must be on the
	// corpus-wide table, like ModuleUID and References.TargetUID already are.
	if data(a.Calls[0].CalleeUID) != data(b.Calls[0].CalleeUID) {
		t.Error("CalleeUID referenced by two files is still two allocations")
	}
	if data(a.Inheritance[1].ParentUID) != data(b.Inheritance[1].ParentUID) {
		t.Error("Inheritance.ParentUID referenced by two files is still two allocations")
	}
	if data(a.FieldAccess[1].FieldUID) != data(b.FieldAccess[1].FieldUID) {
		t.Error("FieldAccess.FieldUID referenced by two files is still two allocations")
	}

	// CallerUID and SourceUID are declared IN the referencing file, not pointed at from
	// elsewhere — the local (per-file) table is correct for them, and unrelated distinct
	// UIDs must never collapse into the same allocation regardless of which table holds them.
	if data(a.Calls[0].CallerUID) == data(b.Calls[0].CallerUID) {
		t.Error("two DIFFERENT CallerUIDs were folded into one value")
	}
}

// A field whose values are all distinct must not grow the table without bound: past the
// cap the interner stops recording and simply hands the string back.
func TestInternerStopsGrowingAtItsLimit(t *testing.T) {
	si := newShardInterner(2)
	first, second := si.of("a"), si.of("b")
	if first != "a" || second != "b" {
		t.Fatalf("interner changed a value: %q %q", first, second)
	}
	if got := si.of("c"); got != "c" {
		t.Errorf("of(c) = %q, want the value handed back unchanged", got)
	}
	if len(si.values) != 2 {
		t.Errorf("table grew to %d entries past its limit of 2", len(si.values))
	}
	if si.of("a") != "a" {
		t.Error("an already-interned value stopped resolving once the table filled")
	}
}

func TestInternerLeavesTheEmptyStringAlone(t *testing.T) {
	si := newShardInterner(8)
	if got := si.of(""); got != "" {
		t.Errorf("of(\"\") = %q", got)
	}
	if len(si.values) != 0 {
		t.Errorf("the empty string took a table slot")
	}
}

func TestNilInternerIsAPassthrough(t *testing.T) {
	var si *shardInterner
	if got := si.of("x"); got != "x" {
		t.Errorf("of(x) on a nil interner = %q", got)
	}
}

func TestClipReleasesSpareCapacityAndKeepsTheValues(t *testing.T) {
	s := make([]int, 3, 64)
	s[0], s[1], s[2] = 7, 8, 9

	got := clip(s)
	if len(got) != 3 || got[0] != 7 || got[1] != 8 || got[2] != 9 {
		t.Fatalf("clip changed the values: %v", got)
	}
	if cap(got) != 3 {
		t.Errorf("cap = %d, want the slack released", cap(got))
	}
	if full := clip([]int{1, 2}); cap(full) != 2 {
		t.Errorf("a slice with no slack was copied anyway")
	}
	if empty := clip(make([]int, 0, 16)); empty != nil {
		t.Errorf("an empty slice kept a 16-element allocation")
	}
}

func data(s string) uintptr {
	return uintptr(unsafe.Pointer(unsafe.StringData(s)))
}

// compactionCorpus exercises every record kind the compaction touches, with values that
// repeat within a file and across files.
func compactionCorpus() []*parseCacheEntry {
	build := func(rel, name string) *parseCacheEntry {
		return &parseCacheEntry{
			RelPath:  rel,
			Language: "go",
			Cluster:  "core",
			DirPaths: []string{"pkg"},
			Entities: []cachedEntity{
				{
					Label: "Function", UID: rel + ":A", Name: "A" + name, Path: rel,
					Line: 1, EndLine: 4, Docstring: "does a thing", Lang: "go",
					Complexity: 2, Context: "pkg", ContextType: "package",
					IsExported: true, Decorators: []string{"deprecated"},
					Args: []string{"ctx"}, Value: "",
				},
				{
					Label: "Function", UID: rel + ":B", Name: "B" + name, Path: rel,
					Line: 6, EndLine: 9, Lang: "go", Context: "pkg",
					ContextType: "package", IsDep: true,
				},
			},
			Parameters: []cachedParameter{
				{UID: rel + ":A:ctx", Name: "ctx", FuncUID: rel + ":A", Path: rel, Line: 1, Lang: "go"},
			},
			Fields: []cachedField{
				{UID: rel + ":A:f", Name: "f", ParentUID: rel + ":A", ParentType: "Struct", Path: rel, Line: 2, Lang: "go"},
			},
			Calls: []cachedCall{
				{CallerUID: rel + ":A", CalleeUID: "fmt.Errorf", SourceType: "Function", Line: 2, Path: rel, ReceiverType: "", Lang: "go"},
				{CallerUID: rel + ":A", CalleeUID: rel + ":B", SourceType: "Function", Line: 3, Path: rel, ReceiverType: "T", Lang: "go"},
			},
			Imports: []cachedImport{
				{FileUID: rel, ModuleUID: "mod:fmt", ModuleName: "fmt", RawImport: `"fmt"`, Alias: "", ImportedName: "fmt", Line: 1, Lang: "go", SourceFile: rel},
			},
			Inheritance: []cachedInheritance{
				{ChildUID: rel + ":A", ParentUID: rel + ":B", RelType: "INHERITS", Path: rel, Line: 1},
				// A base class both files inherit from — the cross-file case ParentUID
				// interning exists for.
				{ChildUID: rel + ":A", ParentUID: "external:CommonBase", RelType: "INHERITS", Path: rel, Line: 2},
			},
			FieldAccess: []cachedFieldAccess{
				{SourceUID: rel + ":A", FieldUID: rel + ":A:f", IsWrite: true, Path: rel, Line: 2},
				// A field both files read — the cross-file case FieldUID interning exists for.
				{SourceUID: rel + ":A", FieldUID: "external:CommonBase.f", IsWrite: false, Path: rel, Line: 3},
			},
			References: []cachedReference{
				{SourceUID: rel + ":A", TargetUID: "PEDIDO", RelType: "SELECTS", Path: rel, Line: 3, Lang: "go"},
			},
			ContainsEdges: []cachedContainsEdge{
				{ParentUID: rel, ChildUID: rel + ":A", ParentLabel: "File", ChildLabel: "Function"},
				{ParentUID: rel, ChildUID: rel + ":B", ParentLabel: "File", ChildLabel: "Function"},
			},
		}
	}
	return []*parseCacheEntry{build("pkg/one.go", "One"), build("pkg/two.go", "Two")}
}
