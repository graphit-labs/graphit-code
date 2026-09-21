//go:build lancedb

package task

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"testing"
	"time"
)

func TestExplicitTaskSessionReferencesReopenPreserveAndClear(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	svc := OpenAt("project-a", uri)
	refs := []relations.Ref{{Target: relations.Entity{Type: "memory", ID: "memory-id"}, Relation: "supports"}}
	input := testCreate("Persist typed references", "typed-refs")
	input.Actor = "worker"
	task, err := svc.Create(relations.WithInputs(ctx, &refs), input)
	if err != nil {
		t.Fatal(err)
	}
	session, err := svc.SessionCreate(relations.WithInputs(ctx, &refs), SessionCreateInput{Title: "Session", Description: "Tracks the explicit references", Strategy: "Reopen and verify", Actor: "agent"})
	if err != nil {
		t.Fatal(err)
	}
	svc = OpenAt("project-a", uri)
	assert := func(want int) {
		t.Helper()
		edges, complete, err := svc.ReferenceEdges(ctx)
		if err != nil || !complete || len(edges) != want {
			t.Fatalf("edges=%+v complete=%v err=%v", edges, complete, err)
		}
	}
	assert(2)
	claimed, err := svc.Claim(ctx, task.ID, input.Actor, time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = svc.Progress(ctx, task.ID, claimed.ClaimToken, input.Actor, "Continued without replacing links", "Validate", time.Hour); err != nil {
		t.Fatal(err)
	}
	sc, err := svc.SessionClaim(ctx, session.ID, "agent", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = svc.SessionCheckpoint(ctx, sc.ID, sc.ClaimToken, "agent", SessionCheckpointInput{Summary: "Checkpoint", NextStep: "Verify"}, time.Hour); err != nil {
		t.Fatal(err)
	}
	assert(2)
	empty := []relations.Ref{}
	if _, err = svc.Progress(relations.WithInputs(ctx, &empty), task.ID, claimed.ClaimToken, input.Actor, "Cleared explicitly", "Verify", time.Hour); err != nil {
		t.Fatal(err)
	}
	if _, err = svc.SessionCheckpoint(relations.WithInputs(ctx, &empty), sc.ID, sc.ClaimToken, "agent", SessionCheckpointInput{Summary: "Cleared", NextStep: "Verify"}, time.Hour); err != nil {
		t.Fatal(err)
	}
	assert(0)
	if err = svc.ReconcileReferences(ctx); err != nil {
		t.Fatal(err)
	}
	assert(0)
	// A typed input error must not mutate the authoritative record.
	bad := []relations.Ref{{Target: relations.Entity{Type: "memory"}}}
	if _, err = svc.Progress(relations.WithInputs(ctx, &bad), task.ID, claimed.ClaimToken, input.Actor, "Bad", "Bad", time.Hour); err == nil {
		t.Fatal("invalid target accepted")
	}
	assert(0)
}
