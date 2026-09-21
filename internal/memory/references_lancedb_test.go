//go:build lancedb

package memory

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"testing"
)

func TestExplicitMemoryReferencesSurviveReopenBodyEditsAndClear(t *testing.T) {
	ctx := context.Background()
	svc := newLocalService(t)
	refs := []relations.Ref{{Target: relations.Entity{Type: "task", ID: "tsk-existing"}, Relation: "derived_from"}}
	id, err := svc.WithContext(relations.WithInputs(ctx, &refs)).AddMemory("Decision", "A durable decision", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	assert := func(want int) {
		t.Helper()
		edges, complete, err := svc.ReferenceEdges(ctx)
		if err != nil || !complete || len(edges) != want {
			t.Fatalf("%+v complete=%v err=%v", edges, complete, err)
		}
	}
	// Every service operation closes and reopens its native table.
	assert(1)
	if err = svc.WithContext(ctx).UpdateMemory(id, "Updated decision", "The revised body"); err != nil {
		t.Fatal(err)
	}
	assert(1)
	if err = svc.ReconcileReferences(ctx); err != nil {
		t.Fatal(err)
	}
	assert(1)
	empty := []relations.Ref{}
	if err = svc.WithContext(relations.WithInputs(ctx, &empty)).UpdateMemory(id, "", ""); err != nil {
		t.Fatal(err)
	}
	assert(0)
	if err = svc.WithContext(ctx).UpdateMemory(id, "", "A further edit"); err != nil {
		t.Fatal(err)
	}
	assert(0)
}
