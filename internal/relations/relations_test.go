//go:build lancedb

package relations

import (
	"context"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"testing"
)

func TestPersistedTypedRelationsSurviveReopenAndOlderWriters(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	open := func() *lancestore.Store {
		s, e := lancestore.Open(ctx, lancestore.Config{URI: dir, Writable: true})
		if e != nil {
			t.Fatal(e)
		}
		return s
	}
	st := open()
	source := Entity{Type: "task", ID: "tsk-123", Scope: "project", ScopeID: "project-a"}
	memory := Ref{Target: Entity{Type: "memory", ID: "01MEM", Scope: "project", ScopeID: "project-a"}, Relation: "references"}
	if e := Replace(ctx, st, source, 1, "record", []Ref{memory}); e != nil {
		t.Fatal(e)
	}
	st.Close()
	st = open()
	defer st.Close()
	edges, initialized, e := Read(ctx, st)
	if e != nil || !initialized || len(edges) != 1 || edges[0].Target.Type != "memory" || edges[0].Target.ID != "01MEM" {
		t.Fatalf("%+v %v %v", edges, initialized, e)
	}
	if e := Replace(ctx, st, source, 2, "record", nil); e != nil {
		t.Fatal(e)
	}
	if e := Replace(ctx, st, source, 1, "record", []Ref{memory}); e != nil {
		t.Fatal(e)
	}
	edges, _, e = Read(ctx, st)
	if e != nil || len(edges) != 0 {
		t.Fatalf("old reference resurrected: %+v %v", edges, e)
	}
}

func TestPruneSupersededKeepsCurrentMarkersAndRejectsDelayedWriter(t *testing.T) {
	ctx := context.Background()
	st, err := lancestore.Open(ctx, lancestore.Config{URI: t.TempDir(), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	if removed, err := PruneSuperseded(ctx, st, nil); err != nil || removed != 0 {
		t.Fatalf("missing relation table: removed=%d err=%v", removed, err)
	}
	source := Entity{Type: "knowledge", ID: "a'b"}
	other := Entity{Type: "knowledge", ID: "other"}
	ref := Ref{Target: Entity{Type: "knowledge", ID: "target"}, Relation: "wiki-link"}
	for _, write := range []struct {
		source   Entity
		revision int64
		refs     []Ref
	}{
		{source, 1, []Ref{ref}},
		{source, 2, nil},
		{other, 1, []Ref{ref}},
	} {
		if err := Replace(ctx, st, write.source, write.revision, "record", write.refs); err != nil {
			t.Fatal(err)
		}
	}
	removed, err := PruneSuperseded(ctx, st, map[string]string{source.Key(): "", other.Key(): ""})
	if err != nil || removed != 2 {
		t.Fatalf("prune: removed=%d err=%v, want two old rows", removed, err)
	}
	table, err := st.OpenTable(ctx, TableName)
	if err != nil {
		t.Fatal(err)
	}
	rows, err := table.Rows(ctx)
	_ = table.Close()
	if err != nil || len(rows) != 3 {
		t.Fatalf("rows after prune: %d, err=%v, want latest empty marker and other generation", len(rows), err)
	}
	if err := Replace(ctx, st, source, 1, "record", []Ref{ref}); err != nil {
		t.Fatal(err)
	}
	edges, complete, err := Read(ctx, st)
	if err != nil || !complete || len(edges) != 1 || edges[0].Source.ID != other.ID {
		t.Fatalf("delayed writer restored a link: edges=%+v complete=%v err=%v", edges, complete, err)
	}
}

func TestPruneSupersededPreservesNanosecondRevisionAfterFloatProjection(t *testing.T) {
	ctx := context.Background()
	st, err := lancestore.Open(ctx, lancestore.Config{URI: t.TempDir(), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	source := Entity{Type: "knowledge", ID: "page"}
	ref := Ref{Target: Entity{Type: "knowledge", ID: "target"}, Relation: "wiki-link"}
	const latest = int64(1789969839338262785)
	if err := Replace(ctx, st, source, latest-1_000_000, "record", []Ref{ref}); err != nil {
		t.Fatal(err)
	}
	if err := Replace(ctx, st, source, latest, "record", nil); err != nil {
		t.Fatal(err)
	}
	if removed, err := PruneSuperseded(ctx, st, map[string]string{source.Key(): ""}); err != nil || removed != 2 {
		t.Fatalf("prune nanosecond revisions: removed=%d err=%v", removed, err)
	}
	table, err := st.OpenTable(ctx, TableName)
	if err != nil {
		t.Fatal(err)
	}
	rows, err := table.Rows(ctx)
	_ = table.Close()
	if err != nil || len(rows) != 1 {
		t.Fatalf("latest marker lost after float projection: rows=%d err=%v", len(rows), err)
	}
}

func TestPruneSupersededKeepsHeadDuringUncommittedGeneration(t *testing.T) {
	ctx := context.Background()
	st, err := lancestore.Open(ctx, lancestore.Config{URI: t.TempDir(), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	source := Entity{Type: "memory", ID: "page"}
	ref := Ref{Target: Entity{Type: "task", ID: "target"}, Relation: "supports"}
	if err := Replace(ctx, st, source, 1, "record", []Ref{ref}, "committed"); err != nil {
		t.Fatal(err)
	}
	if err := Replace(ctx, st, source, 2, "record", nil, "uncommitted"); err != nil {
		t.Fatal(err)
	}
	if removed, err := PruneSuperseded(ctx, st, nil); err != nil || removed != 0 {
		t.Fatalf("missing head pruned relation rows: removed=%d err=%v", removed, err)
	}
	if removed, err := PruneSuperseded(ctx, st, map[string]string{source.Key(): "committed"}); err != nil || removed != 0 {
		t.Fatalf("uncommitted generation pruned the current head: removed=%d err=%v", removed, err)
	}
	edges, complete, err := ReadMatching(ctx, st, map[string]string{source.Key(): "committed"})
	if err != nil || !complete || len(edges) != 1 || edges[0].Target.ID != "target" {
		t.Fatalf("current head lost after incomplete write: edges=%+v complete=%v err=%v", edges, complete, err)
	}
	if removed, err := PruneSuperseded(ctx, st, map[string]string{source.Key(): "uncommitted"}); err != nil || removed != 2 {
		t.Fatalf("committed new head did not prune old rows: removed=%d err=%v", removed, err)
	}
	table, err := st.OpenTable(ctx, TableName)
	if err != nil {
		t.Fatal(err)
	}
	rows, err := table.Rows(ctx)
	_ = table.Close()
	if err != nil || len(rows) != 1 {
		t.Fatalf("new head marker not retained: rows=%d err=%v", len(rows), err)
	}
}

func TestPruneSupersededBatchesManySources(t *testing.T) {
	ctx := context.Background()
	st, err := lancestore.Open(ctx, lancestore.Config{URI: t.TempDir(), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	heads := make(map[string]string)
	for i := range 33 {
		source := Entity{Type: "knowledge", ID: fmt.Sprintf("page-%d", i)}
		heads[source.Key()] = ""
		if err := Replace(ctx, st, source, 1, "record", nil); err != nil {
			t.Fatal(err)
		}
		if err := Replace(ctx, st, source, 2, "record", nil); err != nil {
			t.Fatal(err)
		}
	}
	if removed, err := PruneSuperseded(ctx, st, heads); err != nil || removed != 33 {
		t.Fatalf("batched prune: removed=%d err=%v", removed, err)
	}
	table, err := st.OpenTable(ctx, TableName)
	if err != nil {
		t.Fatal(err)
	}
	rows, err := table.Rows(ctx)
	_ = table.Close()
	if err != nil || len(rows) != 33 {
		t.Fatalf("batched prune rows=%d err=%v", len(rows), err)
	}
}
