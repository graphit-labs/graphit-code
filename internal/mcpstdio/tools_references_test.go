package mcpstdio

import (
	"context"
	"encoding/json"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/references"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"github.com/graphit-labs/graphit-code/internal/task"
	"strings"
	"testing"
)

func TestReferenceWriteSchemasAndAgentInstructions(t *testing.T) {
	client := testMCPClient(t)
	listed, err := client.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	wanted := map[string]bool{}
	for _, action := range []string{"task_create", "task_revise", "task_progress", "task_comment_add", "task_check", "task_complete", "task_session_create", "task_session_revise", "task_session_checkpoint", "task_session_complete", "memory_insert", "memory_update"} {
		wanted[brand.MCPToolName(action)] = true
	}
	for _, tool := range listed.Tools {
		if !wanted[tool.Name] {
			continue
		}
		data, _ := json.Marshal(tool.InputSchema)
		var schema struct{ Properties map[string]json.RawMessage }
		if err := json.Unmarshal(data, &schema); err != nil {
			t.Fatal(err)
		}
		if _, ok := schema.Properties["references"]; !ok {
			t.Errorf("%s missing structured references", tool.Name)
		}
		for _, name := range []string{"target", "type", "id", "relation"} {
			if !strings.Contains(string(data), "\""+name+"\"") {
				t.Errorf("%s missing nested %s contract", tool.Name, name)
			}
		}
		delete(wanted, tool.Name)
	}
	for name := range wanted {
		t.Errorf("tool missing %s", name)
	}
	for name, body := range map[string]string{"task": task.RuleContent(), "memory": memory.RuleContent(nil), "knowledge": knowledge.KnowledgeRuleContent(nil, "")} {
		if !strings.Contains(body, relations.AgentContract) {
			t.Errorf("%s skill omits mandatory explicit relation contract", name)
		}
	}
	for name, body := range map[string]string{"task": task.MandateTrigger(), "worker": task.WorkerMandateTrigger(), "memory": memory.MandateTrigger(), "knowledge": knowledge.MandateTrigger()} {
		if !strings.Contains(body, relations.AgentMandate) {
			t.Errorf("%s hook omits explicit relations", name)
		}
	}
}

func TestReferenceQueryDefaultsAndCursorIdentity(t *testing.T) {
	in := referencesQueryInput{IncludeUser: true}
	f := references.Filter{TargetType: "memory", TargetID: "m"}
	rows := make([]relations.Edge, 101)
	for i := 0; i < 5; i++ {
		window, err := referenceQueryWindow(in, "project", "user-a", f)
		if err != nil {
			t.Fatal(err)
		}
		got := page.Finish(window, rows)
		if len(got.Results) != 20 {
			t.Fatalf("default page has %d rows", len(got.Results))
		}
		if i == 0 {
			other := in
			other.Cursor = got.NextCursor
			if _, err := referenceQueryWindow(other, "project", "user-b", f); err == nil {
				t.Fatal("personal cursor changed user")
			}
			contextName := ""
			different := f
			different.TargetContext = &contextName
			if _, err := referenceQueryWindow(other, "project", "user-a", different); err == nil {
				t.Fatal("cursor changed target namespace filter")
			}
		}
		in.Cursor = got.NextCursor
	}
	if in.Cursor != "" {
		t.Fatal("default 100 result cap ignored")
	}
}
