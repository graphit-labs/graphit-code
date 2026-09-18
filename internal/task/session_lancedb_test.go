//go:build lancedb

package task

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"sync"
	"testing"
	"time"
)

func createSessionFixture(t *testing.T, s *Service, actor, key string) Session {
	t.Helper()
	v, err := s.SessionCreate(context.Background(), SessionCreateInput{Title: "Deliver " + key, Description: "User needs reliable delivery with explicit scope, constraints and acceptance. Inspect current code, plan concrete work, preserve evidence and complete only after validation.", Strategy: "Inspect source and contracts, implement independent tasks, validate integrated behavior.", Actor: actor, IdempotencyKey: key})
	if err != nil {
		t.Fatal(err)
	}
	return v
}
func claimSessionFixture(t *testing.T, s *Service, v Session, actor string) Session {
	t.Helper()
	v, err := s.SessionClaim(context.Background(), v.ID, actor, time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	return v
}
func finishTaskFixture(t *testing.T, s *Service, id, actor string) {
	t.Helper()
	ctx := context.Background()
	v, err := s.Claim(ctx, id, actor, time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	for _, c := range v.Checks {
		if _, err := s.VerifyCheck(ctx, id, v.ClaimToken, actor, c.ID, true, "Fixture exercised expected delivery behavior and documented contract.", time.Hour); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := s.Complete(ctx, id, v.ClaimToken, actor, "Verified task fixture and documentation."); err != nil {
		t.Fatal(err)
	}
}

func TestSessionLifecycleRelationsAndColdHandoff(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	s := OpenAt("session-lifecycle", uri)
	session := claimSessionFixture(t, s, createSessionFixture(t, s, "coordinator", "feature"), "coordinator")
	// Session coordination and task execution are separate claims.
	in := testCreate("Plan scoped work", "plan")
	in.Actor = "coordinator"
	in.RequireSession = true
	task, err := s.Create(ctx, in)
	if err != nil {
		t.Fatal(err)
	}
	if task.SessionID != session.ID {
		t.Fatal("coordinator session not inherited")
	}
	if _, err = s.SessionComplete(ctx, session.ID, session.ClaimToken, "coordinator", "Complete"); err == nil {
		t.Fatal("closed with open task")
	}
	worker := testCreate("Implement scoped work", "work")
	worker.Actor = "worker"
	worker.ParentID = task.ID
	worker.RequireSession = true
	child, err := s.Create(ctx, worker)
	if err != nil {
		t.Fatal(err)
	}
	if child.SessionID != session.ID {
		t.Fatal("parent session not inherited")
	}
	if _, err = s.SessionCheckpoint(ctx, session.ID, session.ClaimToken, "coordinator", SessionCheckpointInput{Summary: "Plan saved; implementation pending.", Problems: "Dependency contract required clarification.", Decisions: "Keep verified current API envelope.", Strategy: "Validate backend before consumers.", NextStep: "Implement task " + child.ID + " and run its checks."}, time.Hour); err != nil {
		t.Fatal(err)
	}
	detail, err := s.SessionGet(ctx, session.ID)
	if err != nil {
		t.Fatal(err)
	}
	revised := detail.Session.Description + " User additionally requires historical traceability."
	if _, err = s.SessionRevise(ctx, session.ID, session.ClaimToken, "coordinator", SessionReviseInput{ExpectedRevision: detail.Session.Revision, Reason: "User expanded the acceptance scope.", Description: &revised}, time.Hour); err != nil {
		t.Fatal(err)
	}
	if _, err = s.SessionRelease(ctx, session.ID, session.ClaimToken, "coordinator", "Plan and revised scope saved; backend work still pending.", "Read associated tasks, finish implementation and validation."); err != nil {
		t.Fatal(err)
	}
	// A fresh service/agent can reconstruct intent, decisions and next action.
	resumed := OpenAt("session-lifecycle", uri)
	detail, err = resumed.SessionGet(ctx, session.ID)
	if err != nil {
		t.Fatal(err)
	}
	if detail.Session.Status != StatusOpen || detail.Session.ClaimToken != "" || len(detail.Tasks) != 2 || len(detail.Checkpoints) != 2 || len(detail.SpecRevisions) != 2 || !strings.Contains(detail.Session.Description, "traceability") {
		t.Fatalf("incomplete handoff: %#v", detail)
	}
	next := claimSessionFixture(t, resumed, detail.Session, "replacement")
	if _, err = resumed.SessionCheckpoint(ctx, session.ID, session.ClaimToken, "coordinator", SessionCheckpointInput{Summary: "stale", NextStep: "stale"}, time.Hour); !errors.Is(err, ErrFence) {
		t.Fatalf("stale token: %v", err)
	}
	finishTaskFixture(t, resumed, child.ID, "worker")
	finishTaskFixture(t, resumed, task.ID, "replacement")
	done, err := resumed.SessionComplete(ctx, next.ID, next.ClaimToken, "replacement", "Current user demand and both tasks verified; no unresolved work. Documentation matches tests.")
	if err != nil {
		t.Fatal(err)
	}
	if done.Status != StatusCompleted || done.ClaimToken != "" {
		t.Fatalf("closure: %#v", done)
	}
	in.IdempotencyKey = "after-closure"
	in.SessionID = session.ID
	if _, err = resumed.Create(ctx, in); err == nil {
		t.Fatal("created work in terminal session")
	}
	export, err := resumed.Export(ctx, "")
	if err != nil {
		t.Fatal(err)
	}
	encoded, _ := json.Marshal(export)
	if len(export.Sessions) != 1 || len(export.SessionCheckpoints) != 2 || len(export.SessionSpecRevisions) != 2 || strings.Contains(string(encoded), next.ClaimToken) || strings.Contains(string(encoded), session.ClaimToken) {
		t.Fatal("export lost session history or leaked token")
	}
}

func TestSessionTaskAssociationAndBatch(t *testing.T) {
	ctx := context.Background()
	s := OpenAt("association", t.TempDir())
	in := testCreate("manual", "manual")
	in.Actor = "manual"
	manual, err := s.Create(ctx, in)
	if err != nil || manual.SessionID != "" {
		t.Fatalf("optional manual: %v %#v", err, manual)
	}
	in.IdempotencyKey = "agent-unbound"
	in.RequireSession = true
	if _, err = s.Create(ctx, in); err == nil {
		t.Fatal("agent task without session accepted")
	}
	a := createSessionFixture(t, s, "agent", "a")
	b := createSessionFixture(t, s, "agent", "b")
	in.SessionID = a.ID
	in.IdempotencyKey = "parent"
	parent, err := s.Create(ctx, in)
	if err != nil {
		t.Fatal(err)
	}
	in.ParentID = parent.ID
	in.SessionID = b.ID
	in.IdempotencyKey = "wrong-session"
	if _, err = s.Create(ctx, in); err == nil {
		t.Fatal("cross-session child accepted")
	}
	batch, err := s.Batch(ctx, BatchInput{Actor: "agent", RequireSession: true, Operations: []BatchOperation{
		{Action: "create", Key: "a", SessionID: a.ID, Title: "Sibling A", Description: "Implement contract A with current evidence and verify its result.", AcceptanceCriteria: []string{"Result A must match contract."}, Tests: []string{"Exercise contract A and inspect expected result."}, IdempotencyKey: "sibling-a"},
		{Action: "create", Key: "bad", Title: "Unassociated", Description: "Missing session intentionally.", AcceptanceCriteria: []string{"Require association."}, Tests: []string{"Reject."}, IdempotencyKey: "sibling-bad"},
	}})
	if err != nil || batch.Succeeded != 1 || batch.Failed != 1 {
		t.Fatalf("batch = %#v %v", batch, err)
	}
	retry := testCreate("Retry wrong session", "parent")
	retry.Actor = "agent"
	retry.RequireSession = true
	retry.SessionID = b.ID
	if _, err := s.Create(ctx, retry); err == nil {
		t.Fatal("idempotency key silently crossed sessions")
	}
	retry.IdempotencyKey = "manual"
	retry.SessionID = a.ID
	if _, err := s.Create(ctx, retry); err == nil {
		t.Fatal("manual key bypassed agent association")
	}
	claimSessionFixture(t, s, b, "inferred")
	retry.IdempotencyKey = "parent"
	retry.Actor = "inferred"
	retry.SessionID = ""
	if _, err := s.Create(ctx, retry); err == nil {
		t.Fatal("inferred coordinator reused another session key")
	}
	list, err := s.List(ctx, ListOptions{SessionID: a.ID})
	if err != nil || len(list) != 2 {
		t.Fatalf("session task filter: %#v %v", list, err)
	}
	other := testCreate("Unique searchablecapability", "outside")
	other.SessionID = b.ID
	other.Actor = "agent"
	if _, err = s.Create(ctx, other); err != nil {
		t.Fatal(err)
	}
	other.IdempotencyKey = "inside"
	other.SessionID = a.ID
	target, err := s.Create(ctx, other)
	if err != nil {
		t.Fatal(err)
	}
	hits, err := s.SearchInSession(ctx, "searchablecapability", 1, a.ID)
	if err != nil || len(hits) != 1 || hits[0].ID != target.ID {
		t.Fatalf("prelimit scope: %#v %v", hits, err)
	}
}

func TestSessionFencingExpiryTakeoverAndCancel(t *testing.T) {
	ctx := context.Background()
	s := OpenAt("fencing", t.TempDir())
	now := time.Date(2026, 9, 17, 0, 0, 0, 0, time.UTC)
	s.now = func() time.Time { return now }
	v := claimSessionFixture(t, s, createSessionFixture(t, s, "a", "one"), "a")
	another := createSessionFixture(t, s, "a", "two")
	if _, err := s.SessionClaim(ctx, another.ID, "a", time.Hour); err == nil {
		t.Fatal("two coordinator claims accepted")
	}
	if _, err := s.SessionClaim(ctx, v.ID, "b", time.Hour); !errors.Is(err, ErrClaimed) {
		t.Fatalf("concurrent claim: %v", err)
	}
	if _, err := s.SessionRevise(ctx, v.ID, v.ClaimToken, "a", SessionReviseInput{ExpectedRevision: v.Revision + 1, Reason: "stale"}, time.Hour); !errors.Is(err, ErrConcurrent) {
		t.Fatalf("revision fence: %v", err)
	}
	changed, err := s.SessionForceTakeover(ctx, v.ID, "b", ForceTakeoverInput{ExpectedRevision: v.Revision, ConfirmID: v.ID, Reason: "Previous owner disconnected; user authorized recovery."}, time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if changed.ClaimToken == v.ClaimToken || changed.ClaimEpoch != 2 {
		t.Fatal("takeover did not rotate fencing")
	}
	now = now.Add(2 * time.Minute)
	if _, err = s.SessionHeartbeat(ctx, v.ID, changed.ClaimToken, "b", time.Minute); !errors.Is(err, ErrFence) {
		t.Fatalf("expired owner writes: %v", err)
	}
	replacement, err := s.SessionClaim(ctx, v.ID, "c", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	in := testCreate("cancel pending", "cancel")
	in.SessionID = v.ID
	in.Actor = "worker"
	task, err := s.Create(ctx, in)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = s.SessionCancel(ctx, v.ID, replacement.ClaimToken, "c", "User cancelled request."); err == nil {
		t.Fatal("cancel silently abandoned pending task")
	}
	if _, err = s.Cancel(ctx, task.ID, "", "worker", "User cancelled this task."); err != nil {
		t.Fatal(err)
	}
	cancelled, err := s.SessionCancel(ctx, v.ID, replacement.ClaimToken, "c", "User cancelled request; linked task explicitly cancelled.")
	if err != nil || cancelled.Status != StatusCancelled {
		t.Fatalf("cancel = %#v %v", cancelled, err)
	}
}

func TestSessionHistoryRepairAndSearch(t *testing.T) {
	ctx := context.Background()
	s := OpenAt("history", t.TempDir())
	v := claimSessionFixture(t, s, createSessionFixture(t, s, "a", "history"), "a")
	first, err := s.SessionCheckpoint(ctx, v.ID, v.ClaimToken, "a", SessionCheckpointInput{Summary: "Investigated retry boundary.", Problems: "Rare quartzfailure on transport.", Decisions: "Keep original idempotency key.", NextStep: "Validate recovery."}, time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	// Model a crash after CAS: remove its materializations while retaining the snapshot.
	if err = s.withTables(ctx, func(tables *tables) error {
		if err := tables.sessionEvents.DeleteByKey(ctx, "key", []string{first.LastEvent.Key}); err != nil {
			return err
		}
		return tables.sessionCheckpoints.DeleteByKey(ctx, "key", []string{first.LastEvent.Key})
	}); err != nil {
		t.Fatal(err)
	}
	detail, err := s.SessionGet(ctx, v.ID)
	if err != nil || len(detail.Checkpoints) != 1 {
		t.Fatalf("read recovery: %#v %v", detail, err)
	}
	if _, err = s.SessionHeartbeat(ctx, v.ID, v.ClaimToken, "a", time.Hour); err != nil {
		t.Fatal(err)
	}
	if _, err = s.SessionCheckpoint(ctx, v.ID, v.ClaimToken, "a", SessionCheckpointInput{Summary: "Issue resolved; current phase changed.", NextStep: "Finish verification."}, time.Hour); err != nil {
		t.Fatal(err)
	}
	detail, err = s.SessionGet(ctx, v.ID)
	if err != nil || len(detail.Checkpoints) != 2 || !strings.Contains(detail.Checkpoints[0].Problems, "quartzfailure") {
		t.Fatalf("historical checkpoint lost: %#v %v", detail, err)
	}
	hits, err := s.SessionSearch(ctx, "quartzfailure", 5)
	if err != nil || len(hits) != 1 || hits[0].ID != v.ID {
		t.Fatalf("historical search: %#v %v", hits, err)
	}
	// Create retry returns the same identity and does not silently revise its intent.
	again, err := s.SessionCreate(ctx, SessionCreateInput{Actor: "a", Title: "Changed", Description: "Changed", Strategy: "Changed", IdempotencyKey: "history"})
	if err != nil || again.ID != v.ID || again.Title == "Changed" || again.ClaimToken != "" {
		t.Fatalf("idempotency/redaction: %#v %v", again, err)
	}
}

func TestSessionConcurrentClaimHasOneCoordinator(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	s := OpenAt("race", uri)
	v := createSessionFixture(t, s, "creator", "race")
	var wg sync.WaitGroup
	results := make(chan error, 2)
	for _, actor := range []string{"a", "b"} {
		wg.Add(1)
		go func(actor string) {
			defer wg.Done()
			_, err := OpenAt("race", uri).SessionClaim(ctx, v.ID, actor, time.Hour)
			results <- err
		}(actor)
	}
	wg.Wait()
	close(results)
	wins := 0
	for err := range results {
		if err == nil {
			wins++
		} else if !errors.Is(err, ErrClaimed) {
			t.Fatalf("unexpected claim error: %v", err)
		}
	}
	if wins != 1 {
		t.Fatalf("coordinator winners = %d", wins)
	}
}

func TestSessionEmptyDiscoveryAndCurrentStrategy(t *testing.T) {
	ctx := context.Background()
	s := OpenAt("empty-discovery", t.TempDir())
	empty, err := s.SessionSearch(ctx, "missing-demand", 5)
	if err != nil || len(empty) != 0 {
		t.Fatalf("empty search: %#v %v", empty, err)
	}
	list, err := s.SessionList(ctx, SessionListOptions{Active: true})
	if err != nil || len(list) != 0 {
		t.Fatalf("empty active list: %#v %v", list, err)
	}
	v := claimSessionFixture(t, s, createSessionFixture(t, s, "a", "strategy"), "a")
	if _, err = s.SessionCheckpoint(ctx, v.ID, v.ClaimToken, "a", SessionCheckpointInput{Summary: "Evidence gathered.", Strategy: "Historical strategy observation.", NextStep: "Decide whether to revise current strategy."}, time.Hour); err != nil {
		t.Fatal(err)
	}
	detail, err := s.SessionGet(ctx, v.ID)
	if err != nil {
		t.Fatal(err)
	}
	if detail.Session.Strategy != v.Strategy || detail.Checkpoints[0].Strategy != "Historical strategy observation." {
		t.Fatal("checkpoint silently revised current strategy")
	}
	strategy := "New current strategy based on gathered evidence."
	updated, err := s.SessionRevise(ctx, v.ID, v.ClaimToken, "a", SessionReviseInput{ExpectedRevision: detail.Session.Revision, Reason: "Evidence warrants changed approach.", Strategy: &strategy}, time.Hour)
	if err != nil || updated.Strategy != strategy {
		t.Fatalf("current strategy: %#v %v", updated, err)
	}
}
