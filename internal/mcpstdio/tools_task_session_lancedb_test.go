//go:build lancedb

package mcpstdio

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func sessionFixtureCall(t *testing.T, c *mcp.ClientSession, dir, action, actor string, fields map[string]any, wantError bool) []byte {
	t.Helper()
	args := map[string]any{"project_dir": dir, "ai_optimized": false}
	for k, v := range fields {
		args[k] = v
	}
	if actor != "" {
		args["agent_id"] = actor
	}
	result, err := c.CallTool(context.Background(), &mcp.CallToolParams{Name: brand.MCPToolName("task", action), Arguments: args})
	failed := err != nil || result != nil && result.IsError
	if failed != wantError {
		t.Fatalf("%s failed=%v want=%v err=%v result=%+v", action, failed, wantError, err, result)
	}
	if wantError {
		return nil
	}
	for _, content := range result.Content {
		if text, ok := content.(*mcp.TextContent); ok && json.Valid([]byte(text.Text)) {
			return []byte(text.Text)
		}
	}
	t.Fatalf("%s returned no JSON", action)
	return nil
}
func fixtureDecode[T any](t *testing.T, b []byte) T {
	t.Helper()
	var value T
	if err := json.Unmarshal(b, &value); err != nil {
		t.Fatal(err)
	}
	return value
}

func TestTaskSessionMCPRoundTripAndFences(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	dir := t.TempDir()
	client := testMCPClient(t)
	call := func(action, actor string, fields map[string]any) []byte {
		return sessionFixtureCall(t, client, dir, action, actor, fields, false)
	}
	deny := func(action, actor string, fields map[string]any) {
		sessionFixtureCall(t, client, dir, action, actor, fields, true)
	}
	created := fixtureDecode[graphtask.Session](t, call("session_create", "coordinator-a", map[string]any{"title": "Archived export delivery", "description": "Detailed request: preserve access controls and default active filtering while exporting archived rows with duplicate delivery prevention.", "strategy": "Inspect contracts then split filter, retry and integrated verification tasks.", "idempotency_key": "mcp-session-one"}))
	claimed := fixtureDecode[graphtask.Session](t, call("session_claim", "coordinator-a", map[string]any{"id": created.ID, "lease": "2h"}))
	if claimed.ClaimToken == "" {
		t.Fatal("claim omitted token")
	}
	taskInput := func(key string) map[string]any {
		return map[string]any{"title": "Worker archival validation " + key, "description": "Verify archived export selection without changing defaults or access boundaries.", "acceptance_criteria": []string{"Archived filter returns only authorized archived records."}, "tests": []string{"Given active and archived fixtures, filter selects only the archived record."}, "idempotency_key": key}
	}
	deny("create", "unassociated-worker", taskInput("missing"))
	worker := fixtureDecode[graphtask.Task](t, call("create", "coordinator-a", taskInput("worker-one")))
	if worker.SessionID != created.ID {
		t.Fatal("coordinator session not inherited")
	}
	badBatch := fixtureDecode[graphtask.BatchResult](t, call("batch", "unassociated-worker", map[string]any{"operations": []map[string]any{{"action": "create", "title": "Missing session", "description": "Detailed fixture specification", "acceptance_criteria": []string{"Valid outcome"}, "tests": []string{"Validate outcome"}}}}))
	if badBatch.Failed != 1 {
		t.Fatal("MCP batch accepted unassociated creation")
	}
	operation := taskInput("worker-two")
	operation["action"] = "create"
	operation["session_id"] = created.ID
	batch := fixtureDecode[graphtask.BatchResult](t, call("batch", "worker-b", map[string]any{"operations": []map[string]any{operation}}))
	if batch.Succeeded != 1 {
		t.Fatalf("explicit session batch failed: %+v", batch)
	}
	cp := fixtureDecode[graphtask.Session](t, call("session_checkpoint", "coordinator-a", map[string]any{"id": created.ID, "claim_token": claimed.ClaimToken, "summary": "Nebulouscheckpoint: filter fixture reproduces the boundary failure.", "problems": "Retry identifier is generated too late.", "decisions": "Preserve existing job ID; add a caller key before submission.", "strategy": "Investigate retry boundary before implementation.", "next_step": "Read the worker task and revise its retry contract before implementing."}))
	detail := fixtureDecode[graphtask.SessionDetail](t, call("session_get", "", map[string]any{"id": created.ID}))
	if detail.Session.ClaimToken != "" || len(detail.Checkpoints) != 1 || len(detail.Tasks) != 2 {
		t.Fatalf("bad detail: %+v", detail)
	}
	if detail.Session.Strategy != created.Strategy {
		t.Fatal("checkpoint silently replaced current strategy")
	}
	revised := fixtureDecode[graphtask.Session](t, call("session_revise", "coordinator-a", map[string]any{"id": created.ID, "claim_token": claimed.ClaimToken, "expected_revision": cp.Revision, "reason": "User added a stable CSV column requirement.", "description": created.Description + " CSV columns remain stable.", "strategy": "Validate filter, retry and CSV contracts before closure."}))
	deny("session_revise", "coordinator-a", map[string]any{"id": created.ID, "claim_token": claimed.ClaimToken, "expected_revision": cp.Revision, "reason": "Stale update must fail.", "strategy": "Wrong stale approach."})
	history := fixtureDecode[page.Page[graphtask.SessionSearchResult]](t, call("session_search", "", map[string]any{"query": "Nebulouscheckpoint", "top_k": 5, "page_size": 1}))
	if len(history.Results) != 1 || history.Results[0].ID != created.ID {
		t.Fatalf("history not searchable: %+v", history)
	}
	second := fixtureDecode[graphtask.Session](t, call("session_create", "coordinator-b", map[string]any{"title": "Separate request", "description": "Separate domain request requiring independent scope and evidence.", "strategy": "Investigate and validate separately.", "idempotency_key": "mcp-session-two"}))
	firstPage := fixtureDecode[page.Page[graphtask.SessionSummary]](t, call("session_list", "", map[string]any{"active": true, "page_size": 1}))
	if len(firstPage.Results) != 1 || firstPage.NextCursor == "" {
		t.Fatalf("no list cursor: %+v", firstPage)
	}
	nextPage := fixtureDecode[page.Page[graphtask.SessionSummary]](t, call("session_list", "", map[string]any{"active": true, "page_size": 1, "cursor": firstPage.NextCursor}))
	if len(nextPage.Results) != 1 || nextPage.Results[0].ID == firstPage.Results[0].ID {
		t.Fatal("list did not advance")
	}
	deny("session_list", "", map[string]any{"active": false, "page_size": 1, "cursor": firstPage.NextCursor})
	list := fixtureDecode[[]graphtask.Task](t, call("list", "", map[string]any{"session_id": created.ID}))
	if len(list) != 2 {
		t.Fatalf("session tasks=%d", len(list))
	}
	absent := fixtureDecode[page.Page[graphtask.SearchResult]](t, call("search", "", map[string]any{"session_id": second.ID, "query": "Worker", "top_k": 5}))
	if len(absent.Results) != 0 {
		t.Fatal("task search leaked another session")
	}
	matches := fixtureDecode[page.Page[graphtask.SearchResult]](t, call("search", "", map[string]any{"session_id": created.ID, "query": "Worker", "top_k": 5}))
	if len(matches.Results) != 2 {
		t.Fatalf("task session search=%+v", matches)
	}
	heartbeat := fixtureDecode[graphtask.Session](t, call("session_heartbeat", "coordinator-a", map[string]any{"id": created.ID, "claim_token": claimed.ClaimToken, "lease": "3h"}))
	if heartbeat.Revision <= revised.Revision {
		t.Fatal("heartbeat did not advance")
	}
	deny("session_force_takeover", "coordinator-c", map[string]any{"id": created.ID, "confirm_id": created.ID, "expected_revision": heartbeat.Revision, "reason": "A recovery request must specify its positive replacement lease.", "lease": ""})
	takeover := fixtureDecode[graphtask.Session](t, call("session_force_takeover", "coordinator-c", map[string]any{"id": created.ID, "confirm_id": created.ID, "expected_revision": heartbeat.Revision, "reason": "Original coordinator process is unrecoverable; fixture recovery.", "lease": "1h"}))
	if takeover.ClaimToken == claimed.ClaimToken {
		t.Fatal("takeover reused token")
	}
	deny("session_checkpoint", "coordinator-a", map[string]any{"id": created.ID, "claim_token": claimed.ClaimToken, "summary": "Stale write.", "next_step": "Must reject."})
	call("session_release", "coordinator-c", map[string]any{"id": created.ID, "claim_token": takeover.ClaimToken, "summary": "Filter evidence saved; both worker fixtures await resolution.", "next_step": "Resolve worker tasks and validate final scope before closing."})
	resume := fixtureDecode[graphtask.Session](t, call("session_claim", "coordinator-d", map[string]any{"id": created.ID}))
	deny("session_complete", "coordinator-d", map[string]any{"id": created.ID, "claim_token": resume.ClaimToken, "summary": "Cannot finish while work remains."})
	deny("session_cancel", "coordinator-d", map[string]any{"id": created.ID, "claim_token": resume.ClaimToken, "reason": "Cannot cancel unfinished associated work."})
	for _, id := range []string{worker.ID, batch.Results[0].ID} {
		call("cancel", "worker-b", map[string]any{"id": id, "reason": "Fixture cancels implementation scope explicitly; no delivery claimed."})
	}
	completed := fixtureDecode[graphtask.Session](t, call("session_complete", "coordinator-d", map[string]any{"id": created.ID, "claim_token": resume.ClaimToken, "summary": "Investigation and handoff verified; implementation fixture tasks were explicitly cancelled. History, filters and ownership evidence preserved."}))
	if completed.Status != graphtask.StatusCompleted {
		t.Fatal("session not completed")
	}
	closedInput := taskInput("closed")
	closedInput["session_id"] = created.ID
	deny("create", "worker-b", closedInput)
	secondClaim := fixtureDecode[graphtask.Session](t, call("session_claim", "coordinator-b", map[string]any{"id": second.ID}))
	cancelled := fixtureDecode[graphtask.Session](t, call("session_cancel", "coordinator-b", map[string]any{"id": second.ID, "claim_token": secondClaim.ClaimToken, "reason": "Separate fixture request is obsolete without associated work."}))
	if cancelled.Status != graphtask.StatusCancelled {
		t.Fatal("session not cancelled")
	}
	// Export never accepts ai_optimized: call the actual interface separately.
	export, err := client.CallTool(context.Background(), &mcp.CallToolParams{Name: brand.MCPToolName("task", "export"), Arguments: map[string]any{"project_dir": dir}})
	if err != nil || export.IsError {
		t.Fatalf("export failed: %v %+v", err, export)
	}
	for _, content := range export.Content {
		if text, ok := content.(*mcp.TextContent); ok && json.Valid([]byte(text.Text)) {
			if strings.Contains(text.Text, claimed.ClaimToken) || strings.Contains(text.Text, resume.ClaimToken) {
				t.Fatal("export leaked claim token")
			}
			var data map[string]any
			if err := json.Unmarshal([]byte(text.Text), &data); err != nil {
				t.Fatal(err)
			}
			if data["schema_version"] != float64(2) || len(data["sessions"].([]any)) != 2 {
				t.Fatal("export omitted sessions")
			}
		}
	}
}
