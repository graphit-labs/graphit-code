package mcpstdio

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func TestTaskSessionToolSchemas(t *testing.T) {
	client := testMCPClient(t)
	listed, err := client.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	required := map[string][]string{
		"create": {"project_dir", "title", "description", "strategy"}, "get": {"project_dir", "id"}, "list": {"project_dir"}, "search": {"project_dir", "query"}, "claim": {"project_dir", "id"},
		"revise": {"project_dir", "id", "claim_token", "expected_revision", "reason"}, "checkpoint": {"project_dir", "id", "claim_token", "summary", "next_step"}, "heartbeat": {"project_dir", "id", "claim_token"},
		"release": {"project_dir", "id", "claim_token", "summary", "next_step"}, "complete": {"project_dir", "id", "claim_token", "summary"}, "cancel": {"project_dir", "id", "claim_token", "reason"}, "force_takeover": {"project_dir", "id", "confirm_id", "expected_revision", "reason", "lease"},
	}
	for _, tool := range listed.Tools {
		action := strings.TrimPrefix(tool.Name, brand.MCPToolName("task", "session")+"_")
		fields, ok := required[action]
		if !ok {
			continue
		}
		b, err := json.Marshal(tool.InputSchema)
		if err != nil {
			t.Fatal(err)
		}
		var schema struct {
			Properties map[string]any
			Required   []string
		}
		if err = json.Unmarshal(b, &schema); err != nil {
			t.Fatal(err)
		}
		for _, name := range fields {
			found := false
			for _, field := range schema.Required {
				if field == name {
					found = true
				}
			}
			if !found {
				t.Errorf("%s must require %s", tool.Name, name)
			}
		}
		if _, ok := schema.Properties["ai_optimized"]; !ok {
			t.Errorf("%s missing ai_optimized", tool.Name)
		}
		if action == "list" || action == "search" {
			for _, name := range []string{"page_size", "cursor"} {
				if _, ok := schema.Properties[name]; !ok {
					t.Errorf("%s missing %s", tool.Name, name)
				}
			}
		}
		delete(required, action)
	}
	for name := range required {
		t.Errorf("session tool not registered: %s", name)
	}
	for _, action := range []string{"create", "list", "search"} {
		found := false
		for _, tool := range listed.Tools {
			if tool.Name != brand.MCPToolName("task", action) {
				continue
			}
			b, _ := json.Marshal(tool.InputSchema)
			var schema struct{ Properties map[string]any }
			_ = json.Unmarshal(b, &schema)
			_, found = schema.Properties["session_id"]
		}
		if !found {
			t.Errorf("task_%s missing session_id", action)
		}
	}
}

type fakeSessionSearcher struct {
	rows   []graphtask.SessionSearchResult
	limits []int
}

func (s *fakeSessionSearcher) SessionSearch(_ context.Context, _ string, limit int) ([]graphtask.SessionSearchResult, error) {
	s.limits = append(s.limits, limit)
	if limit > len(s.rows) {
		limit = len(s.rows)
	}
	return s.rows[:limit], nil
}

func TestTaskSessionSearchPagination(t *testing.T) {
	searcher := &fakeSessionSearcher{rows: []graphtask.SessionSearchResult{{SessionSummary: graphtask.SessionSummary{ID: "ses-a"}}, {SessionSummary: graphtask.SessionSummary{ID: "ses-b"}}, {SessionSummary: graphtask.SessionSummary{ID: "ses-c"}}}}
	in := taskSessionSearchInput{ProjectDir: "/project", Query: "decision", TopK: 3, PageSize: 1}
	var ids []string
	first, err := paginateTaskSessionSearch(context.Background(), searcher, in)
	if err != nil {
		t.Fatal(err)
	}
	cursor := first.NextCursor
	for {
		page, err := paginateTaskSessionSearch(context.Background(), searcher, in)
		if err != nil {
			t.Fatal(err)
		}
		for _, v := range page.Results {
			ids = append(ids, v.ID)
		}
		if page.NextCursor == "" {
			break
		}
		in.Cursor = page.NextCursor
	}
	if strings.Join(ids, ",") != "ses-a,ses-b,ses-c" {
		t.Fatalf("rows=%v", ids)
	}
	for _, change := range []func(*taskSessionSearchInput){func(i *taskSessionSearchInput) { i.Query = "other" }, func(i *taskSessionSearchInput) { i.ProjectDir = "/other" }, func(i *taskSessionSearchInput) { i.PageSize = 2 }, func(i *taskSessionSearchInput) { i.TopK = 4 }} {
		bad := taskSessionSearchInput{ProjectDir: "/project", Query: "decision", TopK: 3, PageSize: 1, Cursor: cursor}
		change(&bad)
		if _, err := paginateTaskSessionSearch(context.Background(), searcher, bad); err == nil {
			t.Fatalf("mismatched cursor accepted: %#v", bad)
		}
	}
}
