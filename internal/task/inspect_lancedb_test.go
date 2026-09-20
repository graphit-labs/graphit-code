//go:build lancedb

package task

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// inspectFixture builds three tasks in distinguishable states, plus a claimed session, so the
// redaction tests have a live token to fail on rather than an empty column.
func inspectFixture(t *testing.T) (*Service, map[string]string) {
	t.Helper()
	ctx := context.Background()
	svc := OpenAt("inspect", t.TempDir())

	session := createSessionFixture(t, svc, "coordinator", "inspect-session")
	if _, err := svc.SessionClaim(ctx, session.ID, "coordinator", time.Hour); err != nil {
		t.Fatalf("claim session: %v", err)
	}

	states := map[string]string{}
	for _, spec := range []struct{ title, key, want string }{
		{"Stay open", "inspect-open", "open"},
		{"Be claimed", "inspect-progress", "in_progress"},
		{"Be finished", "inspect-done", "completed"},
	} {
		in := testCreate(spec.title, spec.key)
		in.Actor = "coordinator"
		in.SessionID = session.ID
		created, err := svc.Create(ctx, in)
		if err != nil {
			t.Fatalf("create %s: %v", spec.key, err)
		}
		switch spec.want {
		case "in_progress":
			if _, err := svc.Claim(ctx, created.ID, "worker-holding", time.Hour); err != nil {
				t.Fatalf("claim %s: %v", spec.key, err)
			}
		case "completed":
			finishTaskFixture(t, svc, created.ID, "worker-finishing")
		}
		states[created.ID] = spec.want
	}
	return svc, states
}

// TK-T3: the schema is read from the store, and every table is there.
func TestDescribeStoreReportsEveryTaskTable(t *testing.T) {
	svc, states := inspectFixture(t)

	schema, err := svc.DescribeStore(context.Background(), nil)
	if err != nil {
		t.Fatalf("describe: %v", err)
	}
	got := map[string]lancequery.TableInfo{}
	for _, tbl := range schema.Tables {
		got[tbl.Name] = tbl
	}
	for _, want := range StoreTables() {
		if _, ok := got[want]; !ok {
			t.Errorf("table %s missing from the described store", want)
		}
	}
	if len(got) != len(StoreTables()) {
		t.Fatalf("described %d tables, the store owns %d", len(got), len(StoreTables()))
	}
	if n := got[tasksTableName].Rows; n != int64(len(states)) {
		t.Fatalf("tasks row count = %d, want %d", n, len(states))
	}

	// The three tiers must be visible in the description, or a caller cannot tell why a
	// column is missing from a default projection.
	flags := map[string]lancequery.Column{}
	for _, c := range got[tasksTableName].Columns {
		flags[c.Name] = c
	}
	if !flags["claim_token"].Redacted {
		t.Error("claim_token must be described as redacted")
	}
	if !flags["description"].Heavy {
		t.Error("description must be described as heavy")
	}
	if flags["status"].Redacted || flags["status"].Heavy {
		t.Error("status must stay an ordinary column")
	}
	if !got[sessionsTableName].Columns[snapshotIndex(t, got[sessionsTableName].Columns)].Redacted {
		t.Error("task_sessions.snapshot_json must be described as redacted")
	}

	// Narrowing must work, and must refuse a name that is not a table.
	one, err := svc.DescribeStore(context.Background(), []string{tasksTableName})
	if err != nil {
		t.Fatalf("describe one: %v", err)
	}
	if len(one.Tables) != 1 || one.Tables[0].Name != tasksTableName {
		t.Fatalf("narrowing returned %d tables", len(one.Tables))
	}
	if _, err := svc.DescribeStore(context.Background(), []string{"no_such_table"}); err == nil {
		t.Fatal("describing a table that does not exist succeeded")
	}
}

func snapshotIndex(t *testing.T, cols []lancequery.Column) int {
	t.Helper()
	for i, c := range cols {
		if c.Name == "snapshot_json" {
			return i
		}
	}
	t.Fatal("task_sessions has no snapshot_json column; the redaction policy is now pointing at nothing")
	return 0
}

// TK-T1: the question that started the demand, answered in one call.
func TestQueryStoreAnswersStatusOfAKnownIDSetInOneCall(t *testing.T) {
	svc, states := inspectFixture(t)

	ids := make([]string, 0, len(states))
	for id := range states {
		ids = append(ids, "'"+id+"'")
	}
	res, err := svc.QueryStore(context.Background(), lancequery.Request{
		Table:   tasksTableName,
		Filter:  fmt.Sprintf("id IN (%s)", strings.Join(ids, ",")),
		Columns: []string{"id", "status"},
	})
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(res.Rows) != len(states) {
		t.Fatalf("returned %d rows, want %d", len(res.Rows), len(states))
	}
	for _, row := range res.Rows {
		if len(row) != 2 {
			t.Fatalf("projection returned extra columns: %v", keysOf(row))
		}
		id := fmt.Sprint(row["id"])
		if got, want := fmt.Sprint(row["status"]), states[id]; got != want {
			t.Errorf("%s status = %q, want %q", id, got, want)
		}
	}
	// Paging is pagination's job now, not this layer's: three rows under the default limit
	// come back as three rows, with no envelope of their own.
	if res.Mode != "filter" {
		t.Errorf("mode = %q, want filter", res.Mode)
	}
}

// TK-T2: the fencing token is unreachable from both directions.
func TestQueryStoreRefusesTheClaimToken(t *testing.T) {
	svc, _ := inspectFixture(t)
	ctx := context.Background()

	for _, tc := range []struct {
		name string
		req  lancequery.Request
	}{
		{"projection", lancequery.Request{Table: tasksTableName, Columns: []string{"id", "claim_token"}}},
		{"filter", lancequery.Request{Table: tasksTableName, Filter: "claim_token != ''"}},
		{"control lock token", lancequery.Request{Table: controlTableName, Columns: []string{"token"}}},
		{"session snapshot", lancequery.Request{Table: sessionsTableName, Columns: []string{"snapshot_json"}}},
		{"session snapshot in filter", lancequery.Request{Table: sessionsTableName, Filter: "snapshot_json LIKE '%x%'"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := svc.QueryStore(ctx, tc.req); err == nil {
				t.Fatal("a redacted column was accepted")
			}
		})
	}

	// And it is absent from a default projection, which is the path a caller takes without
	// thinking about redaction at all.
	res, err := svc.QueryStore(ctx, lancequery.Request{Table: tasksTableName})
	if err != nil {
		t.Fatalf("default projection: %v", err)
	}
	for _, row := range res.Rows {
		if _, ok := row["claim_token"]; ok {
			t.Fatal("claim_token reached a default projection")
		}
		if _, ok := row["description"]; ok {
			t.Fatal("the heavy description column reached a default projection")
		}
		if _, ok := row["status"]; !ok {
			t.Fatal("status must be in a default projection")
		}
	}

	// The live session token must not be reachable through the snapshot blob either. This is
	// the case that reading the schema alone would have missed: task_sessions has no column
	// named claim_token, yet the token is stored inside snapshot_json.
	sessions, err := svc.QueryStore(ctx, lancequery.Request{Table: sessionsTableName})
	if err != nil {
		t.Fatalf("session query: %v", err)
	}
	for _, row := range sessions.Rows {
		for col, v := range row {
			if strings.Contains(fmt.Sprint(v), "claim_token") {
				t.Fatalf("session column %s still carries the claim token", col)
			}
		}
	}
}

// A heavy column stays available; it is a cost default, not a prohibition.
func TestQueryStoreReturnsHeavyColumnsOnRequest(t *testing.T) {
	svc, _ := inspectFixture(t)
	res, err := svc.QueryStore(context.Background(), lancequery.Request{
		Table: tasksTableName, Columns: []string{"id", "description"}, Limit: 1,
	})
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	if len(res.Rows) != 1 {
		t.Fatalf("got %d rows, want 1", len(res.Rows))
	}
	if body := fmt.Sprint(res.Rows[0]["description"]); !strings.Contains(body, "Objective, scope") {
		t.Fatalf("heavy column came back empty or truncated: %q", body)
	}
}

func keysOf(row map[string]any) []string {
	out := make([]string, 0, len(row))
	for k := range row {
		out = append(out, k)
	}
	return out
}
