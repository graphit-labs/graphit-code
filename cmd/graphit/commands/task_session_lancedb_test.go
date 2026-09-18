//go:build lancedb

package commands

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"testing"

	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func sessionCLI(t *testing.T, stdin string, wantError bool, args ...string) []byte {
	t.Helper()
	cmd := newTaskCmd()
	var out bytes.Buffer
	cmd.SetOut(&out)
	cmd.SetErr(&out)
	cmd.SetIn(strings.NewReader(stdin))
	cmd.SetArgs(args)
	err := cmd.Execute()
	if (err != nil) != wantError {
		t.Fatalf("task %v error=%v wantError=%v output=%s", args, err, wantError, out.String())
	}
	return out.Bytes()
}
func sessionCLIValue[T any](t *testing.T, data []byte) T {
	t.Helper()
	var result T
	if err := json.Unmarshal(data, &result); err != nil {
		t.Fatalf("decoding command JSON: %v %s", err, data)
	}
	return result
}

func TestTaskSessionCLIRealLifecycleAndManualTasks(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	dir := t.TempDir()
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err = os.Chdir(dir); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(cwd) })
	// The actual manual create command may still write an unassociated task.
	sessionCLI(t, "", false, "create", "Manual standalone", "--description", "Self-contained manual request with an observable outcome.", "--acceptance", "The manual outcome is observable", "--test", "Validate manual outcome", "--agent", "manual")
	svc, err := graphtask.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	manual, err := svc.List(context.Background(), graphtask.ListOptions{})
	if err != nil || len(manual) != 1 || manual[0].SessionID != "" {
		t.Fatalf("manual standalone create failed: %+v %v", manual, err)
	}
	initial := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "create", "CLI export request", "--description", "Detailed export request: preserve filtering and authorization and verify retry behavior.", "--strategy", "Inspect contracts, split executable tasks, validate integrated behavior.", "--idempotency-key", "cli-session", "--agent", "cli-a"))
	claim := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "claim", initial.ID, "--agent", "cli-a", "--lease", "2h"))
	if claim.ClaimToken == "" {
		t.Fatal("claim token missing")
	}
	sessionCLI(t, "", false, "create", "Associated CLI worker", "--description", "Validate export filtering and retry behavior with isolated fixtures.", "--acceptance", "Only intended rows are exported", "--test", "Given fixtures verify selected rows", "--session", initial.ID, "--agent", "worker")
	tasks, err := svc.List(context.Background(), graphtask.ListOptions{SessionID: initial.ID})
	if err != nil || len(tasks) != 1 {
		t.Fatalf("associated task missing: %+v %v", tasks, err)
	}
	cp := sessionCLIValue[graphtask.Session](t, sessionCLI(t, `{"summary":"Clidurablecheckpoint: reproduced the retry boundary","problems":"Caller key arrives too late","decisions":"Preserve job ID","strategy":"Inspect boundary","next_step":"Revise retry task with input validation before implementing"}`, false, "session", "checkpoint", initial.ID, "-", "--agent", "cli-a", "--claim-token", claim.ClaimToken))
	revised := sessionCLIValue[graphtask.Session](t, sessionCLI(t, `{"strategy":"Verify CSV and retry contracts before closure"}`, false, "session", "revise", initial.ID, "-", "--agent", "cli-a", "--claim-token", claim.ClaimToken, "--expected-revision", fmt.Sprint(cp.Revision), "--reason", "User clarified CSV requirements"))
	sessionCLI(t, `{"strategy":"Stale strategy"}`, true, "session", "revise", initial.ID, "-", "--agent", "cli-a", "--claim-token", claim.ClaimToken, "--expected-revision", fmt.Sprint(cp.Revision), "--reason", "Must reject stale revision")
	detail := sessionCLIValue[graphtask.SessionDetail](t, sessionCLI(t, "", false, "session", "get", initial.ID))
	if detail.Session.ClaimToken != "" || len(detail.Checkpoints) != 1 || len(detail.Tasks) != 1 {
		t.Fatalf("bad CLI read: %+v", detail)
	}
	found := sessionCLIValue[[]graphtask.SessionSearchResult](t, sessionCLI(t, "", false, "session", "search", "Clidurablecheckpoint", "--limit", "5"))
	if len(found) != 1 || found[0].ID != initial.ID {
		t.Fatal("CLI history search failed")
	}
	listed := sessionCLIValue[[]graphtask.SessionSummary](t, sessionCLI(t, "", false, "session", "list", "--active", "--owner", "cli-a"))
	if len(listed) != 1 || listed[0].ID != initial.ID {
		t.Fatal("CLI list filter failed")
	}
	heart := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "heartbeat", initial.ID, "--agent", "cli-a", "--claim-token", claim.ClaimToken))
	if heart.Revision <= revised.Revision {
		t.Fatal("heartbeat missing")
	}
	sessionCLI(t, "", true, "session", "force-takeover", initial.ID, "--agent", "cli-b", "--confirm-id", initial.ID, "--expected-revision", fmt.Sprint(heart.Revision), "--reason", "Missing replacement lease must fail", "--lease", "")
	take := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "force-takeover", initial.ID, "--agent", "cli-b", "--confirm-id", initial.ID, "--expected-revision", fmt.Sprint(heart.Revision), "--reason", "Coordinator process unrecoverable in fixture", "--lease", "1h"))
	if take.ClaimToken == claim.ClaimToken {
		t.Fatal("takeover failed to rotate token")
	}
	sessionCLI(t, "", true, "session", "heartbeat", initial.ID, "--agent", "cli-a", "--claim-token", claim.ClaimToken)
	sessionCLI(t, "", false, "session", "release", initial.ID, "--agent", "cli-b", "--claim-token", take.ClaimToken, "--summary", "Checkpoint and strategy saved; worker remains open", "--next-step", "Resolve worker before request closure")
	resume := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "claim", initial.ID, "--agent", "cli-c"))
	sessionCLI(t, "", true, "session", "complete", initial.ID, "--agent", "cli-c", "--claim-token", resume.ClaimToken, "--summary", "Must reject outstanding work")
	sessionCLI(t, "", true, "session", "cancel", initial.ID, "--agent", "cli-c", "--claim-token", resume.ClaimToken, "--reason", "Must not silently cancel work")
	sessionCLI(t, "", false, "cancel", tasks[0].ID, "--agent", "worker", "--reason", "Fixture implementation scope explicitly cancelled")
	complete := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "complete", initial.ID, "--agent", "cli-c", "--claim-token", resume.ClaimToken, "--summary", "CLI request lifecycle and handoff validated; worker implementation scope explicitly cancelled, no delivery claimed"))
	if complete.Status != graphtask.StatusCompleted {
		t.Fatal("CLI completion failed")
	}
	other := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "create", "Obsolete CLI request", "--description", "Independent request no longer needed by user", "--strategy", "Review then cancel with no implicit child changes", "--agent", "cli-d"))
	otherClaim := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "claim", other.ID, "--agent", "cli-d"))
	cancelled := sessionCLIValue[graphtask.Session](t, sessionCLI(t, "", false, "session", "cancel", other.ID, "--agent", "cli-d", "--claim-token", otherClaim.ClaimToken, "--reason", "Independent request obsolete; no tasks created"))
	if cancelled.Status != graphtask.StatusCancelled {
		t.Fatal("CLI cancel failed")
	}
}
