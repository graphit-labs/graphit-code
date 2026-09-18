//go:build lancedb

package task

import (
	"context"
	"testing"
	"time"
)

func TestHookLifecyclePreservesSessionCoordinationAndHandoff(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("hook-sessions", t.TempDir())
	now := time.Date(2026, 9, 17, 12, 0, 0, 0, time.UTC)
	svc.now = func() time.Time { return now }
	create := func(actor string) (Session, Task) {
		t.Helper()
		session, err := svc.SessionCreate(ctx, SessionCreateInput{Title: actor, Description: "Deliver the requested feature with verified implementation and documentation.", Strategy: "Inspect contracts, execute tasks, validate, then close explicitly.", IdempotencyKey: actor, Actor: actor})
		if err != nil {
			t.Fatal(err)
		}
		session, err = svc.SessionClaim(ctx, session.ID, actor, time.Hour)
		if err != nil {
			t.Fatal(err)
		}
		session, err = svc.SessionCheckpoint(ctx, session.ID, session.ClaimToken, actor, SessionCheckpointInput{Summary: "Contract verified against implementation.", Problems: "Integration remains pending.", Decisions: "Preserve the existing interface to keep consumers compatible.", Strategy: "Validate before closing.", NextStep: "Run integration fixtures, then check delivery acceptance."}, time.Hour)
		if err != nil {
			t.Fatal(err)
		}
		in := testCreate(actor+" task", actor+"-task")
		in.Actor, in.SessionID = actor, session.ID
		work, err := svc.Create(ctx, in)
		if err != nil {
			t.Fatal(err)
		}
		work, err = svc.Claim(ctx, work.ID, actor, time.Hour)
		if err != nil {
			t.Fatal(err)
		}
		return session, work
	}
	coordinator, ownTask := create("coordinator")
	other, otherTask := create("other")
	workerInput := testCreate("Worker unit", "worker-task")
	workerInput.Actor, workerInput.SessionID = "worker", coordinator.ID
	workerTask, err := svc.Create(ctx, workerInput)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Claim(ctx, workerTask.ID, "worker", time.Hour); err != nil {
		t.Fatal(err)
	}

	now = now.Add(10 * time.Minute)
	if err := svc.HeartbeatOwned(ctx, "coordinator", time.Hour); err != nil {
		t.Fatal(err)
	}
	sessionState, err := svc.SessionGet(ctx, coordinator.ID)
	if err != nil {
		t.Fatal(err)
	}
	workState, err := svc.Get(ctx, ownTask.ID)
	if err != nil {
		t.Fatal(err)
	}
	if sessionState.Session.LeaseExpiresAt <= coordinator.LeaseExpiresAt || workState.Task.LeaseExpiresAt <= ownTask.LeaseExpiresAt {
		t.Fatal("hook did not renew independent coordinator and task claims")
	}
	if err := svc.ReleaseOwned(ctx, "worker"); err != nil {
		t.Fatal(err)
	}
	if err := svc.ReleaseOwned(ctx, ""); err != nil {
		t.Fatal(err)
	}
	sessionState, err = svc.SessionGet(ctx, coordinator.ID)
	if err != nil {
		t.Fatal(err)
	}
	if sessionState.Session.Status != StatusInProgress || sessionState.Session.Owner != "coordinator" {
		t.Fatal("worker/unknown stop released another actor's coordination")
	}
	workerState, err := svc.Get(ctx, workerTask.ID)
	if err != nil || workerState.Task.Status != StatusOpen {
		t.Fatalf("worker stop did not release its task: %v", err)
	}
	for range 2 {
		if err := svc.ReleaseOwned(ctx, "coordinator"); err != nil {
			t.Fatal(err)
		}
	}
	sessionState, err = svc.SessionGet(ctx, coordinator.ID)
	if err != nil {
		t.Fatal(err)
	}
	if sessionState.Session.Status != StatusOpen || sessionState.Session.Owner != "" || sessionState.Session.CompletedAt != "" || sessionState.Session.CheckpointSequence != 1 || len(sessionState.Checkpoints) != 1 || sessionState.Session.ProgressSummary != coordinator.ProgressSummary || sessionState.Session.NextStep != coordinator.NextStep {
		t.Fatalf("stop changed substantive handoff or completed the session: %+v", sessionState.Session)
	}
	workState, err = svc.Get(ctx, ownTask.ID)
	if err != nil || workState.Task.Status != StatusOpen {
		t.Fatalf("coordinator stop did not release its task: %v", err)
	}
	otherState, err := svc.SessionGet(ctx, other.ID)
	if err != nil || otherState.Session.Revision != other.Revision || otherState.Session.Status != StatusInProgress {
		t.Fatalf("unrelated coordination changed: %v", err)
	}
	otherWork, err := svc.Get(ctx, otherTask.ID)
	if err != nil || otherWork.Task.Revision != otherTask.Revision || otherWork.Task.Status != StatusInProgress {
		t.Fatalf("unrelated task changed: %v", err)
	}
	now = now.Add(2 * time.Hour)
	if err := svc.Reconcile(ctx); err != nil {
		t.Fatal(err)
	}
	otherState, err = svc.SessionGet(ctx, other.ID)
	if err != nil || otherState.Session.Status != StatusOpen || otherState.Session.CheckpointSequence != 1 || otherState.Session.NextStep != other.NextStep {
		t.Fatalf("expired coordination cannot be resumed from its saved handoff: %v", err)
	}
	otherWork, err = svc.Get(ctx, otherTask.ID)
	if err != nil || otherWork.Task.Status != StatusOpen {
		t.Fatalf("expired task not reconciled: %v", err)
	}
}
