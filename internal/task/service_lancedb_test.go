//go:build lancedb

package task

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func testCreate(title, key string) CreateInput {
	return CreateInput{
		Title: title, IdempotencyKey: key, Description: "Objective, scope, constraints, and expected result for " + title + ".",
		AcceptanceCriteria: []string{"The intended result is observable"}, Tests: []string{"Run the relevant verification"},
	}
}

func TestDeterministicLifecycleDependenciesAndFencing(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-1", t.TempDir())
	now := time.Date(2026, 9, 3, 12, 0, 0, 0, time.UTC)
	svc.now = func() time.Time { return now }

	firstInput := testCreate("Prepare schema", "schema")
	firstInput.Actor, firstInput.Priority = "planner", 1
	first, err := svc.Create(ctx, firstInput)
	if err != nil {
		t.Fatalf("create first: %v", err)
	}
	againInput := testCreate("A duplicate title is ignored", "schema")
	againInput.Actor, againInput.Priority = "planner", 4
	again, err := svc.Create(ctx, againInput)
	if err != nil {
		t.Fatalf("idempotent create: %v", err)
	}
	if again.ID != first.ID || again.Title != first.Title {
		t.Fatalf("idempotent create returned %#v, want original %#v", again, first)
	}

	secondInput := testCreate("Use schema", "consumer")
	secondInput.DependsOn, secondInput.Actor, secondInput.Priority = []string{first.ID}, "planner", 0
	second, err := svc.Create(ctx, secondInput)
	if err != nil {
		t.Fatalf("create dependent: %v", err)
	}
	ready, err := svc.List(ctx, ListOptions{Ready: true})
	if err != nil {
		t.Fatalf("ready: %v", err)
	}
	if len(ready) != 1 || ready[0].ID != first.ID {
		t.Fatalf("ready = %#v, want only %s", ready, first.ID)
	}

	claimed, err := svc.Claim(ctx, first.ID, "agent-a", time.Minute)
	if err != nil {
		t.Fatalf("claim: %v", err)
	}
	if claimed.ClaimToken == "" || claimed.ClaimEpoch != 1 {
		t.Fatalf("claim lacks fencing state: %#v", claimed)
	}
	if _, err := svc.Claim(ctx, first.ID, "agent-b", time.Minute); !errors.Is(err, ErrClaimed) {
		t.Fatalf("second claim error = %v, want ErrClaimed", err)
	}

	checkpoint, err := svc.Progress(ctx, first.ID, claimed.ClaimToken, "agent-a", "schema table landed", "run tests", time.Minute)
	if err != nil {
		t.Fatalf("progress: %v", err)
	}
	if checkpoint.ProgressSequence != 1 || checkpoint.NextStep != "run tests" {
		t.Fatalf("checkpoint = %#v", checkpoint)
	}
	released, err := svc.Release(ctx, first.ID, claimed.ClaimToken, "agent-a", "tests pending", "run tests")
	if err != nil {
		t.Fatalf("release: %v", err)
	}
	if released.Status != StatusOpen || released.Owner != "" {
		t.Fatalf("released = %#v", released)
	}

	reclaimed, err := svc.Claim(ctx, first.ID, "agent-b", time.Minute)
	if err != nil {
		t.Fatalf("takeover claim: %v", err)
	}
	if reclaimed.ClaimEpoch != 2 || reclaimed.ClaimToken == claimed.ClaimToken {
		t.Fatalf("takeover did not fence old owner: %#v", reclaimed)
	}
	flagged, err := svc.Flag(ctx, first.ID, reclaimed.ClaimToken, "agent-b", "acceptance test still failing")
	if err != nil {
		t.Fatalf("flag: %v", err)
	}
	if !flagged.Flagged || flagged.FlagReason == "" {
		t.Fatalf("flag not recorded: %#v", flagged)
	}
	if _, err := svc.Complete(ctx, first.ID, reclaimed.ClaimToken, "agent-b", "too early"); err == nil {
		t.Fatal("flagged task was completed")
	}
	flaggedOpen, err := svc.Release(ctx, first.ID, reclaimed.ClaimToken, "agent-b", "flag needs investigation", "resolve the acceptance failure")
	if err != nil || !flaggedOpen.Flagged {
		t.Fatalf("release flagged task: task=%#v err=%v", flaggedOpen, err)
	}
	flaggedClaim, err := svc.Claim(ctx, first.ID, "agent-c", time.Minute)
	if err != nil {
		t.Fatalf("take over flagged work: %v", err)
	}
	if _, err := svc.Unflag(ctx, first.ID, flaggedClaim.ClaimToken, "agent-c"); err != nil {
		t.Fatalf("unflag: %v", err)
	}
	if _, err := svc.Progress(ctx, first.ID, claimed.ClaimToken, "agent-a", "stale write", "", time.Minute); !errors.Is(err, ErrFence) {
		t.Fatalf("stale progress error = %v, want ErrFence", err)
	}
	if _, err := svc.Complete(ctx, first.ID, flaggedClaim.ClaimToken, "agent-c", "acceptance passed"); err == nil {
		t.Fatal("task completed with unchecked acceptance/tests")
	}
	for _, check := range flaggedClaim.Checks {
		if _, err := svc.VerifyCheck(ctx, first.ID, flaggedClaim.ClaimToken, "agent-c", check.ID, true, "verified by test evidence", time.Minute); err != nil {
			t.Fatalf("verify check %s: %v", check.ID, err)
		}
	}
	comment, err := svc.AddComment(ctx, first.ID, flaggedClaim.ClaimToken, "agent-c", "decision", "Kept the schema compatible with existing readers.", "compat-decision", time.Minute)
	if err != nil || comment.Kind != "decision" {
		t.Fatalf("comment: %#v, %v", comment, err)
	}
	if _, err := svc.Complete(ctx, first.ID, flaggedClaim.ClaimToken, "agent-c", "acceptance passed"); err != nil {
		t.Fatalf("complete: %v", err)
	}

	ready, err = svc.List(ctx, ListOptions{Ready: true})
	if err != nil {
		t.Fatalf("ready after complete: %v", err)
	}
	if len(ready) != 1 || ready[0].ID != second.ID {
		t.Fatalf("ready after complete = %#v, want only %s", ready, second.ID)
	}
	detail, err := svc.Get(ctx, first.ID)
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if len(detail.Events) < 6 {
		t.Fatalf("audit has %d events, want lifecycle history", len(detail.Events))
	}
	if len(detail.Comments) != 1 || detail.Comments[0].ID != comment.ID {
		t.Fatalf("comments=%#v", detail.Comments)
	}
	if detail.Task.ClaimToken != "" {
		t.Fatal("task_get exposed a fencing token")
	}
}

func TestConcurrentClaimHasExactlyOneWinner(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	planner := OpenAt("project-claim-race", uri)
	in := testCreate("Exclusively allocated work", "exclusive-claim")
	in.Actor = "planner"
	created, err := planner.Create(ctx, in)
	if err != nil {
		t.Fatal(err)
	}

	type result struct {
		task Task
		err  error
	}
	start := make(chan struct{})
	results := make(chan result, 2)
	for _, actor := range []string{"agent-a", "agent-b"} {
		actor := actor
		go func() {
			<-start
			claimed, claimErr := OpenAt("project-claim-race", uri).Claim(ctx, created.ID, actor, time.Minute)
			results <- result{task: claimed, err: claimErr}
		}()
	}
	close(start)

	winners := 0
	losers := 0
	for range 2 {
		got := <-results
		switch {
		case got.err == nil:
			winners++
			if got.task.Owner != "agent-a" && got.task.Owner != "agent-b" {
				t.Fatalf("unexpected claim owner %q", got.task.Owner)
			}
		case errors.Is(got.err, ErrClaimed):
			losers++
		default:
			t.Fatalf("claim race returned unexpected error: %v", got.err)
		}
	}
	if winners != 1 || losers != 1 {
		t.Fatalf("claim race: winners=%d losers=%d, want one each", winners, losers)
	}
}

func TestBatchRunsEveryItemInOrderAndPreservesLifecycleChecks(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-batch", t.TempDir())
	createResult, err := svc.Batch(ctx, BatchInput{Actor: "agent-a", Operations: []BatchOperation{
		{Key: "first", Action: "create", Title: "Batch-controlled work", Description: "Exercise ordered batch checks and completion without bypassing lifecycle invariants.", AcceptanceCriteria: []string{"The acceptance condition passes"}, Tests: []string{"The targeted test passes"}, IdempotencyKey: "batch-controlled-work"},
		{Key: "second", Action: "create", Title: "Second batch task", Description: "Verify that multiple task specifications can be created in one ordered request.", AcceptanceCriteria: []string{"The task is created"}, Tests: []string{"Read the created task"}, IdempotencyKey: "second-batch-task"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if createResult.Succeeded != 2 || createResult.Failed != 0 || len(createResult.Results) != 2 {
		t.Fatalf("unexpected create batch: %#v", createResult)
	}
	created, ok := createResult.Results[0].Value.(Task)
	if !ok || created.ID == "" || createResult.Results[1].ID == "" {
		t.Fatalf("create batch did not return task identities: %#v", createResult)
	}
	claimed, err := svc.Claim(ctx, created.ID, "agent-a", 2*time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	passed := true
	operations := make([]BatchOperation, 0, len(claimed.Checks)+2)
	for _, check := range claimed.Checks {
		operations = append(operations, BatchOperation{Action: "check", ID: created.ID, ClaimToken: claimed.ClaimToken, CheckID: check.ID, Passed: &passed, Evidence: "verified in the batch test"})
	}
	operations = append(operations,
		BatchOperation{Key: "invalid", Action: "unknown", ID: created.ID},
		BatchOperation{Key: "finish", Action: "complete", ID: created.ID, ClaimToken: claimed.ClaimToken, Summary: "all checks passed"},
	)

	result, err := svc.Batch(ctx, BatchInput{Operations: operations, Actor: "agent-a"})
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Results) != len(operations) || result.Succeeded != len(operations)-1 || result.Failed != 1 {
		t.Fatalf("unexpected batch summary: %#v", result)
	}
	for index, item := range result.Results {
		if item.Index != index {
			t.Fatalf("result %d has index %d", index, item.Index)
		}
	}
	if result.Results[len(result.Results)-2].OK || result.Results[len(result.Results)-2].Error == "" {
		t.Fatalf("invalid operation did not report an explicit failure: %#v", result.Results[len(result.Results)-2])
	}
	if !result.Results[len(result.Results)-1].OK {
		t.Fatalf("completion after an independent failure did not run: %#v", result.Results[len(result.Results)-1])
	}
	detail, err := svc.Get(ctx, created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if detail.Task.Status != StatusCompleted {
		t.Fatalf("task status = %s, want completed", detail.Task.Status)
	}
}

func TestTaskSpecificationRevisionsAndCheckSupersession(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-spec-revisions", t.TempDir())
	created, err := svc.Create(ctx, testCreate("Original title", "spec-revisions"))
	if err != nil {
		t.Fatal(err)
	}
	claimed, err := svc.Claim(ctx, created.ID, "agent-a", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	verified, err := svc.VerifyCheck(ctx, created.ID, claimed.ClaimToken, "agent-a", created.Checks[0].ID, true, "evidence for the original scope", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	description := "A revised self-contained specification with corrected scope."
	revised, err := svc.Revise(ctx, created.ID, claimed.ClaimToken, "agent-a", ReviseInput{
		ExpectedRevision: verified.Revision,
		Reason:           "The implementation scope changed after discovery.",
		Description:      &description,
		AddTests:         []string{"The corrected scope has a focused regression test"},
	}, time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	if revised.Description != description {
		t.Fatalf("description = %q", revised.Description)
	}
	for _, check := range revised.Checks {
		if check.Status != "pending" || check.Evidence != "" {
			t.Fatalf("active check was not reset after semantic revision: %#v", check)
		}
	}
	if _, err := svc.Revise(ctx, created.ID, claimed.ClaimToken, "agent-a", ReviseInput{ExpectedRevision: verified.Revision, Reason: "stale", Description: &description}, time.Hour); !errors.Is(err, ErrConcurrent) {
		t.Fatalf("stale revise error = %v", err)
	}
	if _, err := svc.Revise(ctx, created.ID, "stale-token", "agent-a", ReviseInput{ExpectedRevision: revised.Revision, Reason: "invalid owner", Description: &description}, time.Hour); !errors.Is(err, ErrFence) {
		t.Fatalf("unfenced revise error = %v", err)
	}

	oldCheck := created.Checks[0]
	if _, err := svc.SupersedeCheck(ctx, created.ID, claimed.ClaimToken, "agent-a", SupersedeCheckInput{ExpectedRevision: revised.Revision, CheckID: oldCheck.ID, Reason: "Remove the only active check of its kind"}, time.Hour); err == nil {
		t.Fatal("supersession removed the only active check kind without replacement")
	}
	superseded, err := svc.SupersedeCheck(ctx, created.ID, claimed.ClaimToken, "agent-a", SupersedeCheckInput{
		ExpectedRevision: revised.Revision,
		CheckID:          oldCheck.ID,
		Reason:           "The original wording no longer matches the corrected scope.",
		ReplacementText:  "The corrected scope satisfies the observable requirement",
	}, time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	var obsolete Check
	for _, check := range superseded.Checks {
		if check.ID == oldCheck.ID {
			obsolete = check
		}
	}
	if obsolete.Status != "superseded" || obsolete.SupersededBy != "agent-a" || obsolete.SupersededReason == "" || obsolete.SupersededAt == "" || obsolete.ReplacementCheckID == "" {
		t.Fatalf("superseded check = %#v", obsolete)
	}
	if _, err := svc.VerifyCheck(ctx, created.ID, claimed.ClaimToken, "agent-a", oldCheck.ID, true, "obsolete evidence", time.Hour); err == nil {
		t.Fatal("expected superseded check verification to fail")
	}

	detail, err := svc.Get(ctx, created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if len(detail.SpecRevisions) != 3 {
		t.Fatalf("spec revisions = %d, want 3", len(detail.SpecRevisions))
	}
	if detail.SpecRevisions[1].Before.Description == detail.SpecRevisions[1].After.Description {
		t.Fatal("revision did not preserve distinct before and after specifications")
	}
	if detail.SpecRevisions[2].Kind != "check_superseded" {
		t.Fatalf("last revision kind = %q", detail.SpecRevisions[2].Kind)
	}

	current := superseded
	for _, check := range current.Checks {
		if check.Status == "superseded" {
			continue
		}
		current, err = svc.VerifyCheck(ctx, current.ID, claimed.ClaimToken, "agent-a", check.ID, true, "focused verification passed", time.Hour)
		if err != nil {
			t.Fatal(err)
		}
	}
	if _, err := svc.Complete(ctx, current.ID, claimed.ClaimToken, "agent-a", "active checks passed"); err != nil {
		t.Fatal(err)
	}
}

func TestBatchSupportsReviseAndCheckSupersede(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-batch-revisions", t.TempDir())
	created, err := svc.Create(ctx, testCreate("Batch revision", "batch-revision"))
	if err != nil {
		t.Fatal(err)
	}
	claimed, err := svc.Claim(ctx, created.ID, "agent-a", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	priority := 1
	result, err := svc.Batch(ctx, BatchInput{Actor: "agent-a", Operations: []BatchOperation{
		{Action: "revise", ID: created.ID, ClaimToken: claimed.ClaimToken, ExpectedRevision: claimed.Revision, Reason: "Raise priority after discovery", Priority: &priority},
		{Action: "check_supersede", ID: created.ID, ClaimToken: claimed.ClaimToken, ExpectedRevision: claimed.Revision + 1, CheckID: created.Checks[0].ID, Reason: "Replace obsolete wording", ReplacementText: "The replacement acceptance requirement passes"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if result.Failed != 0 || result.Succeeded != 2 {
		t.Fatalf("batch result = %#v", result)
	}
	detail, err := svc.Get(ctx, created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if detail.Task.Priority != priority || len(detail.SpecRevisions) != 3 {
		t.Fatalf("task = %#v, revisions = %d", detail.Task, len(detail.SpecRevisions))
	}
}

func TestLeaseRenewalNeverShortensAnActiveClaim(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-lease-renewal", t.TempDir())
	now := time.Date(2026, 9, 3, 12, 0, 0, 0, time.UTC)
	svc.now = func() time.Time { return now }
	input := testCreate("Long-running work", "long-running-work")
	input.Actor = "agent-a"
	created, err := svc.Create(ctx, input)
	if err != nil {
		t.Fatal(err)
	}
	claimed, err := svc.Claim(ctx, created.ID, "agent-a", 4*time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	want := claimed.LeaseExpiresAt
	now = now.Add(10 * time.Minute)
	renewed, err := svc.Heartbeat(ctx, created.ID, claimed.ClaimToken, "agent-a", DefaultLease)
	if err != nil {
		t.Fatal(err)
	}
	if renewed.LeaseExpiresAt != want {
		t.Fatalf("heartbeat shortened lease: got %s want %s", renewed.LeaseExpiresAt, want)
	}
	if DefaultLease != time.Hour {
		t.Fatalf("default lease = %s, want 1h", DefaultLease)
	}
}

func TestCompactTaskIDsLengthenOnCollision(t *testing.T) {
	ctx := context.Background()
	projectID := "project-short-id"
	svc := OpenAt(projectID, t.TempDir())
	firstKey, secondKey := collidingTaskKeys(projectID)

	firstInput := testCreate("First collision candidate", firstKey)
	firstInput.Actor = "planner"
	first, err := svc.Create(ctx, firstInput)
	if err != nil {
		t.Fatal(err)
	}
	secondInput := testCreate("Second collision candidate", secondKey)
	secondInput.Actor = "planner"
	second, err := svc.Create(ctx, secondInput)
	if err != nil {
		t.Fatal(err)
	}
	if len(first.ID) != len("tsk-")+taskIDMinHashLength {
		t.Fatalf("first id %q has length %d", first.ID, len(first.ID))
	}
	if first.ID == second.ID || len(second.ID) <= len(first.ID) || !strings.HasPrefix(second.ID, first.ID) {
		t.Fatalf("collision was not lengthened: first=%q second=%q", first.ID, second.ID)
	}

	firstAgain, err := svc.Create(ctx, firstInput)
	if err != nil {
		t.Fatal(err)
	}
	secondAgain, err := svc.Create(ctx, secondInput)
	if err != nil {
		t.Fatal(err)
	}
	if firstAgain.ID != first.ID || secondAgain.ID != second.ID {
		t.Fatalf("idempotent ids changed: first=%q/%q second=%q/%q", first.ID, firstAgain.ID, second.ID, secondAgain.ID)
	}
}

func TestCreateReusesLegacyLongIDByIdempotencyKey(t *testing.T) {
	ctx := context.Background()
	projectID := "project-legacy-id"
	uri := t.TempDir()
	svc := OpenAt(projectID, uri)
	input := testCreate("Legacy task", "legacy-idempotency-key")
	input.Actor = "planner"
	legacyID := "tsk-0123456789ab"
	checks, err := buildChecks(legacyID, input.AcceptanceCriteria, input.Tests)
	if err != nil {
		t.Fatal(err)
	}
	now := stamp(time.Now().UTC())
	legacy := Task{ID: legacyID, ProjectID: projectID, IdempotencyKey: input.IdempotencyKey, Title: input.Title,
		Description: input.Description, Type: "task", Status: StatusOpen, Priority: 2, Checks: checks,
		CreatedAt: now, UpdatedAt: now, Revision: 1}
	legacy.LastEvent = newEvent(legacy, "created", input.Actor, "", StatusOpen, "task created", "")
	tables, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	if err := tables.tasks.Upsert(ctx, "id", []lancestore.Row{taskRow(legacy)}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := svc.projectTask(ctx, tables, legacy, input.Actor); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.close(); err != nil {
		t.Fatal(err)
	}

	got, err := svc.Create(ctx, input)
	if err != nil {
		t.Fatal(err)
	}
	if got.ID != legacyID {
		t.Fatalf("legacy id changed: got %q want %q", got.ID, legacyID)
	}
}

func TestConcurrentCollidingTaskCreationDoesNotOverwrite(t *testing.T) {
	ctx := context.Background()
	projectID := "project-concurrent-short-id"
	uri := t.TempDir()
	firstKey, secondKey := collidingTaskKeys(projectID)
	collisionPrefix := taskIDDigest(projectID, firstKey)[:taskIDMinHashLength]
	seedKey := "initialize-store"
	for taskIDDigest(projectID, seedKey)[:taskIDMinHashLength] == collisionPrefix {
		seedKey += "-next"
	}
	seedInput := testCreate("Initialize task indexes", seedKey)
	seedInput.Actor = "planner"
	if _, err := OpenAt(projectID, uri).Create(ctx, seedInput); err != nil {
		t.Fatal(err)
	}
	type result struct {
		task Task
		err  error
	}
	start := make(chan struct{})
	results := make(chan result, 2)
	for index, key := range []string{firstKey, secondKey} {
		index, key := index, key
		go func() {
			<-start
			input := testCreate(fmt.Sprintf("Concurrent collision %d", index), key)
			input.Actor = fmt.Sprintf("planner-%d", index)
			created, err := OpenAt(projectID, uri).Create(ctx, input)
			results <- result{task: created, err: err}
		}()
	}
	close(start)
	created := make([]Task, 0, 2)
	for range 2 {
		result := <-results
		if result.err != nil {
			t.Fatal(result.err)
		}
		created = append(created, result.task)
	}
	if created[0].ID == created[1].ID {
		t.Fatalf("concurrent collision overwrote task %q", created[0].ID)
	}
	short, long := created[0].ID, created[1].ID
	if len(short) > len(long) {
		short, long = long, short
	}
	if len(short) != len("tsk-")+taskIDMinHashLength || !strings.HasPrefix(long, short) {
		t.Fatalf("unexpected adaptive ids: %q and %q", created[0].ID, created[1].ID)
	}
}

func collidingTaskKeys(projectID string) (string, string) {
	seen := make(map[string]string)
	for i := 0; ; i++ {
		key := fmt.Sprintf("collision-key-%d", i)
		prefix := taskIDDigest(projectID, key)[:taskIDMinHashLength]
		if previous, ok := seen[prefix]; ok {
			return previous, key
		}
		seen[prefix] = key
	}
}

func TestExpiredClaimIsReopenedAndCyclesAreRejected(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-2", t.TempDir())
	now := time.Date(2026, 9, 3, 12, 0, 0, 0, time.UTC)
	svc.now = func() time.Time { return now }
	aInput := testCreate("A", "a")
	aInput.Actor = "planner"
	a, err := svc.Create(ctx, aInput)
	if err != nil {
		t.Fatal(err)
	}
	bInput := testCreate("B", "b")
	bInput.Actor, bInput.DependsOn = "planner", []string{a.ID}
	b, err := svc.Create(ctx, bInput)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = svc.AddDependency(ctx, a.ID, b.ID, "planner"); err == nil {
		t.Fatal("cycle was accepted")
	}
	claim, err := svc.Claim(ctx, a.ID, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	now = now.Add(2 * time.Minute)
	reclaimed, err := svc.Claim(ctx, a.ID, "agent-b", time.Minute)
	if err != nil {
		t.Fatalf("claim expired target: %v", err)
	}
	detail, err := svc.Get(ctx, a.ID)
	if err != nil {
		t.Fatal(err)
	}
	if detail.Task.Status != StatusInProgress || detail.Task.Owner != "agent-b" || detail.Task.ClaimToken != "" {
		t.Fatalf("expired claim not atomically reassigned and redacted: %#v", detail.Task)
	}
	if reclaimed.ClaimToken == "" || reclaimed.ClaimEpoch != 2 {
		t.Fatalf("reclaimed task lacks a new fence: %#v", reclaimed)
	}
	if _, err = svc.Progress(ctx, a.ID, claim.ClaimToken, "agent-a", "late", "", time.Minute); !errors.Is(err, ErrFence) {
		t.Fatalf("expired token error=%v", err)
	}
}

func TestSearchUsesTaskTableText(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-search", t.TempDir())
	auth := testCreate("Repair authentication cache", "auth")
	auth.Description, auth.Actor = "OIDC token invalidation with scope and constraints", "planner"
	_, err := svc.Create(ctx, auth)
	if err != nil {
		t.Fatal(err)
	}
	parser := testCreate("Tune parser", "parser")
	parser.Description, parser.Actor = "ANTLR throughput with measurable expected result", "planner"
	_, err = svc.Create(ctx, parser)
	if err != nil {
		t.Fatal(err)
	}
	hits, err := svc.Search(ctx, "authentication", 10)
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) == 0 || hits[0].Title != "Repair authentication cache" {
		t.Fatalf("hits=%#v", hits)
	}
}

func TestParentCannotCompleteBeforeVerifiedSubtask(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-parent", t.TempDir())
	parentInput := testCreate("Parent", "parent")
	parent, err := svc.Create(ctx, parentInput)
	if err != nil {
		t.Fatal(err)
	}
	childInput := testCreate("Child", "child")
	childInput.ParentID = parent.ID
	child, err := svc.Create(ctx, childInput)
	if err != nil {
		t.Fatal(err)
	}
	children, err := svc.List(ctx, ListOptions{ParentID: parent.ID})
	if err != nil || len(children) != 1 || children[0].ID != child.ID {
		t.Fatalf("children=%#v err=%v", children, err)
	}
	parentClaim, err := svc.Claim(ctx, parent.ID, "parent-agent", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	for _, check := range parentClaim.Checks {
		if _, err := svc.VerifyCheck(ctx, parent.ID, parentClaim.ClaimToken, "parent-agent", check.ID, true, "parent evidence", time.Minute); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := svc.Complete(ctx, parent.ID, parentClaim.ClaimToken, "parent-agent", "done"); err == nil {
		t.Fatal("parent completed before subtask")
	}
	childClaim, err := svc.Claim(ctx, child.ID, "child-agent", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	for _, check := range childClaim.Checks {
		if _, err := svc.VerifyCheck(ctx, child.ID, childClaim.ClaimToken, "child-agent", check.ID, true, "child evidence", time.Minute); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := svc.Complete(ctx, child.ID, childClaim.ClaimToken, "child-agent", "done"); err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Complete(ctx, parent.ID, parentClaim.ClaimToken, "parent-agent", "done"); err != nil {
		t.Fatal(err)
	}
}

func TestCancelRecordsReasonAndFencesLiveOwner(t *testing.T) {
	ctx := context.Background()
	svc := OpenAt("project-cancel", t.TempDir())
	openInput := testCreate("Cancel open work", "cancel-open")
	openInput.Actor = "planner"
	openTask, err := svc.Create(ctx, openInput)
	if err != nil {
		t.Fatal(err)
	}
	cancelled, err := svc.Cancel(ctx, openTask.ID, "", "reviewer", "direction changed")
	if err != nil {
		t.Fatal(err)
	}
	if cancelled.Status != StatusCancelled || cancelled.ProgressSummary != "cancelled: direction changed" {
		t.Fatalf("cancelled task=%#v", cancelled)
	}
	detail, err := svc.Get(ctx, openTask.ID)
	if err != nil {
		t.Fatal(err)
	}
	last := detail.Events[len(detail.Events)-1]
	if last.Type != "cancelled" || last.Summary != "direction changed" || last.Actor != "reviewer" {
		t.Fatalf("cancel event=%#v", last)
	}

	activeInput := testCreate("Cancel active work", "cancel-active")
	activeInput.Actor = "planner"
	active, err := svc.Create(ctx, activeInput)
	if err != nil {
		t.Fatal(err)
	}
	claim, err := svc.Claim(ctx, active.ID, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Cancel(ctx, active.ID, "wrong-token", "agent-a", "obsolete"); !errors.Is(err, ErrFence) {
		t.Fatalf("wrong cancellation fence: %v", err)
	}
	if _, err := svc.Cancel(ctx, active.ID, claim.ClaimToken, "agent-a", "obsolete"); err != nil {
		t.Fatal(err)
	}
}

func TestRemoveRequiresExactConfirmationRejectsReferencesAndCleansProjections(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	svc := OpenAt("project-remove", uri)

	anchorInput := testCreate("Completed anchor", "remove-anchor")
	anchorInput.Actor = "planner"
	anchor, err := svc.Create(ctx, anchorInput)
	if err != nil {
		t.Fatal(err)
	}
	anchorClaim, err := svc.Claim(ctx, anchor.ID, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	for _, check := range anchorClaim.Checks {
		if _, err := svc.VerifyCheck(ctx, anchor.ID, anchorClaim.ClaimToken, "agent-a", check.ID, true, "anchor verified", time.Minute); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := svc.Complete(ctx, anchor.ID, anchorClaim.ClaimToken, "agent-a", "anchor complete"); err != nil {
		t.Fatal(err)
	}

	disposableInput := testCreate("Mistaken disposable work", "remove-disposable")
	disposableInput.Actor = "planner"
	disposableInput.DependsOn = []string{anchor.ID}
	disposable, err := svc.Create(ctx, disposableInput)
	if err != nil {
		t.Fatal(err)
	}
	disposableClaim, err := svc.Claim(ctx, disposable.ID, "agent-b", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	comment, err := svc.AddComment(ctx, disposable.ID, disposableClaim.ClaimToken, "agent-b", "note", "This work was created in error.", "remove-note", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Remove(ctx, disposable.ID, "wrong-id", "reviewer", "created by mistake"); err == nil {
		t.Fatal("remove accepted an inexact confirmation")
	}
	removed, err := svc.Remove(ctx, disposable.ID, disposable.ID, "reviewer", "created by mistake")
	if err != nil {
		t.Fatal(err)
	}
	if removed.ID != disposable.ID || removed.RemovedBy != "reviewer" {
		t.Fatalf("removal=%#v", removed)
	}
	if _, err := svc.Get(ctx, disposable.ID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("removed task get error=%v", err)
	}
	inspect, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	defer inspect.close()
	for name, table := range map[string]*lancestore.Table{"events": inspect.events, "comments": inspect.comments, "checks": inspect.checks, "dependencies": inspect.dependencies, "spec revisions": inspect.specRevisions} {
		hits, err := table.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(disposable.ID), Limit: pageSize})
		if err != nil {
			t.Fatal(err)
		}
		if len(hits) != 0 {
			t.Fatalf("%s retained %d rows", name, len(hits))
		}
	}
	comments, err := inspect.comments.Search(ctx, lancestore.Query{Filter: "id = " + quote(comment.ID), Limit: 1})
	if err != nil || len(comments) != 0 {
		t.Fatalf("comment retained after remove: hits=%d err=%v", len(comments), err)
	}

	baseInput := testCreate("Referenced base", "referenced-base")
	baseInput.Actor = "planner"
	base, err := svc.Create(ctx, baseInput)
	if err != nil {
		t.Fatal(err)
	}
	dependentInput := testCreate("References base", "references-base")
	dependentInput.Actor = "planner"
	dependentInput.DependsOn = []string{base.ID}
	if _, err := svc.Create(ctx, dependentInput); err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Remove(ctx, base.ID, base.ID, "reviewer", "try referenced"); !errors.Is(err, ErrReferenced) {
		t.Fatalf("dependent reference error=%v", err)
	}
	parentInput := testCreate("Referenced parent", "referenced-parent")
	parentInput.Actor = "planner"
	parent, err := svc.Create(ctx, parentInput)
	if err != nil {
		t.Fatal(err)
	}
	childInput := testCreate("Child reference", "child-reference")
	childInput.Actor = "planner"
	childInput.ParentID = parent.ID
	if _, err := svc.Create(ctx, childInput); err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Remove(ctx, parent.ID, parent.ID, "reviewer", "try parent"); !errors.Is(err, ErrReferenced) {
		t.Fatalf("subtask reference error=%v", err)
	}
}

func TestReconcileFinishesInterruptedRemoval(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	svc := OpenAt("project-remove-reconcile", uri)
	input := testCreate("Interrupted removal", "interrupted-removal")
	input.Actor = "planner"
	created, err := svc.Create(ctx, input)
	if err != nil {
		t.Fatal(err)
	}
	tables, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	intent := lancestore.Row{"key": taskRemovalPrefix + created.ID, "token": "confirmed", "owner": "reviewer", "acquired_at": stamp(time.Now().UTC()), "expires_at": "", "revision": int64(1)}
	if err := tables.control.Upsert(ctx, "key", []lancestore.Row{intent}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.close(); err != nil {
		t.Fatal(err)
	}
	if err := svc.Reconcile(ctx); err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Get(ctx, created.ID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("interrupted removal was not finished: %v", err)
	}
}

func TestRemovalIntentBlocksNewReferencesAndReconcileRejectsStaleReferencedIntent(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	svc := OpenAt("project-remove-tombstone", uri)

	baseInput := testCreate("Removal tombstone base", "removal-tombstone-base")
	baseInput.Actor = "planner"
	base, err := svc.Create(ctx, baseInput)
	if err != nil {
		t.Fatal(err)
	}
	tables, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	intentKey := taskRemovalPrefix + base.ID
	intent := lancestore.Row{"key": intentKey, "token": "confirmed", "owner": "reviewer", "acquired_at": stamp(time.Now().UTC()), "expires_at": "", "revision": int64(1)}
	if err := tables.control.Upsert(ctx, "key", []lancestore.Row{intent}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.close(); err != nil {
		t.Fatal(err)
	}

	dependentInput := testCreate("Blocked dependent", "blocked-dependent")
	dependentInput.Actor = "planner"
	dependentInput.DependsOn = []string{base.ID}
	if _, err := svc.Create(ctx, dependentInput); err == nil || !strings.Contains(err.Error(), "pending removal") {
		t.Fatalf("dependency was not blocked by removal intent: %v", err)
	}
	childInput := testCreate("Blocked subtask", "blocked-subtask")
	childInput.Actor = "planner"
	childInput.ParentID = base.ID
	if _, err := svc.Create(ctx, childInput); err == nil || !strings.Contains(err.Error(), "pending removal") {
		t.Fatalf("subtask was not blocked by removal intent: %v", err)
	}

	if err := tablesReopenDeleteIntent(ctx, uri, svc, intentKey); err != nil {
		t.Fatal(err)
	}
	dependentInput.IdempotencyKey = "existing-dependent"
	dependent, err := svc.Create(ctx, dependentInput)
	if err != nil {
		t.Fatal(err)
	}
	tables, err = openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	if err := tables.control.Upsert(ctx, "key", []lancestore.Row{intent}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.close(); err != nil {
		t.Fatal(err)
	}
	if err := svc.Reconcile(ctx); err != nil {
		t.Fatal(err)
	}
	if _, err := svc.Get(ctx, base.ID); err != nil {
		t.Fatalf("referenced task was removed: %v", err)
	}
	if _, err := svc.Get(ctx, dependent.ID); err != nil {
		t.Fatalf("dependent was damaged: %v", err)
	}
	inspect, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	defer inspect.close()
	hits, err := inspect.control.Search(ctx, lancestore.Query{Filter: "key = " + quote(intentKey), Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) != 0 {
		t.Fatal("unsafe removal intent was not cleared")
	}
}

func tablesReopenDeleteIntent(ctx context.Context, uri string, svc *Service, key string) error {
	tables, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		return err
	}
	defer tables.close()
	return tables.control.DeleteByKey(ctx, "key", []string{key})
}

func TestReconcileBypassesSchedulerLeaseAndRepairsProjections(t *testing.T) {
	ctx := context.Background()
	uri := t.TempDir()
	svc := OpenAt("project-reconcile", uri)

	baseInput := testCreate("Projection source", "projection-source")
	baseInput.Actor = "planner"
	base, err := svc.Create(ctx, baseInput)
	if err != nil {
		t.Fatal(err)
	}
	dependentInput := testCreate("Projection dependent", "projection-dependent")
	dependentInput.Actor = "planner"
	dependentInput.DependsOn = []string{base.ID}
	dependent, err := svc.Create(ctx, dependentInput)
	if err != nil {
		t.Fatal(err)
	}
	claimed, err := svc.Claim(ctx, base.ID, "agent-a", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	comment, err := svc.AddComment(ctx, base.ID, claimed.ClaimToken, "agent-a", "decision", "Preserve the authoritative snapshot while repairing projections.", "projection-repair", time.Minute)
	if err != nil {
		t.Fatal(err)
	}

	tables, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	current, ok, err := tables.getTask(ctx, base.ID)
	if err != nil || !ok {
		_ = tables.close()
		t.Fatalf("authoritative task: ok=%v err=%v", ok, err)
	}
	if err := tables.events.DeleteByKey(ctx, "key", []string{current.LastEvent.Key}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.comments.DeleteByKey(ctx, "id", []string{comment.ID}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.dependencies.DeleteByKey(ctx, "key", []string{dependent.ID + "/" + base.ID}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	checkKeys := make([]string, 0, len(current.Checks))
	for _, check := range current.Checks {
		checkKeys = append(checkKeys, check.ID)
	}
	if err := tables.checks.DeleteByKey(ctx, "key", checkKeys); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	lease := lancestore.Row{"key": "scheduler", "token": "interrupted-hook", "owner": "other-agent", "acquired_at": stamp(time.Now().UTC()), "expires_at": stamp(time.Now().UTC().Add(time.Minute)), "revision": int64(1)}
	if err := tables.control.Upsert(ctx, "key", []lancestore.Row{lease}); err != nil {
		_ = tables.close()
		t.Fatal(err)
	}
	if err := tables.close(); err != nil {
		t.Fatal(err)
	}

	reconcileCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	if err := svc.Reconcile(reconcileCtx); err != nil {
		t.Fatalf("reconcile with active scheduler lease: %v", err)
	}

	inspect, err := openTables(ctx, uri, svc.s3)
	if err != nil {
		t.Fatal(err)
	}
	defer inspect.close()
	events, err := inspect.eventsFor(ctx, base.ID)
	if err != nil {
		t.Fatal(err)
	}
	eventFound := false
	for _, event := range events {
		eventFound = eventFound || event.Key == current.LastEvent.Key
	}
	comments, err := inspect.commentsFor(ctx, base.ID)
	if err != nil {
		t.Fatal(err)
	}
	commentFound := false
	for _, got := range comments {
		commentFound = commentFound || got.ID == comment.ID
	}
	dependencyRows, err := inspect.dependencies.Search(ctx, lancestore.Query{Filter: "key = " + quote(dependent.ID+"/"+base.ID), Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	checkRows, err := inspect.checks.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(base.ID), Limit: pageSize})
	if err != nil {
		t.Fatal(err)
	}
	controlRows, err := inspect.control.Search(ctx, lancestore.Query{Filter: "key = 'scheduler'", Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	if !eventFound || !commentFound || len(dependencyRows) != 1 || !boolValue(dependencyRows[0].Row, "active") {
		t.Fatalf("projection repair: event=%v comment=%v dependencies=%#v", eventFound, commentFound, dependencyRows)
	}
	if len(checkRows) != len(current.Checks) {
		t.Fatalf("repaired checks=%d, want %d", len(checkRows), len(current.Checks))
	}
	if len(controlRows) != 1 || text(controlRows[0].Row, "token") != "interrupted-hook" {
		t.Fatalf("reconcile modified scheduler lease: %#v", controlRows)
	}
}
