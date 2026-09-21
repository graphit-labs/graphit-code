//go:build lancedb

package relations

import (
	"context"
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
