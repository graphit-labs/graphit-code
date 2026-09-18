package commands

import (
	"strings"
	"testing"

	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func TestTaskSessionCommandsAndRequiredFlags(t *testing.T) {
	commands := map[string][]string{
		"create": {"description", "strategy"}, "get": {}, "list": {}, "search": {}, "claim": {},
		"revise": {"expected-revision", "reason", "claim-token"}, "checkpoint": {"claim-token"}, "heartbeat": {"claim-token"}, "release": {"claim-token", "summary", "next-step"}, "complete": {"claim-token", "summary"}, "cancel": {"claim-token", "reason"}, "force-takeover": {"confirm-id", "expected-revision", "reason", "lease"},
	}
	for action, required := range commands {
		root := newTaskCmd()
		cmd, _, err := root.Find([]string{"session", action})
		if err != nil || cmd.Name() != action {
			t.Fatalf("missing task session %s: %v", action, err)
		}
		for _, name := range required {
			f := cmd.Flags().Lookup(name)
			if f == nil || len(f.Annotations["cobra_annotation_bash_completion_one_required_flag"]) == 0 {
				t.Errorf("session %s must require --%s", action, name)
			}
		}
	}
	for _, action := range []string{"create", "list", "search"} {
		cmd, _, err := newTaskCmd().Find([]string{action})
		if err != nil || cmd.Flags().Lookup("session") == nil {
			t.Errorf("task %s missing --session", action)
		}
	}
}

func TestTaskSessionJSONInputsAreStrict(t *testing.T) {
	var patch taskSessionRevisionPatch
	if err := decodeStrictTaskJSON(strings.NewReader(`{"description":"Full replacement scope","strategy":"Evidence first"}`), "-", &patch, "session revision"); err != nil || patch.Description == nil || patch.Strategy == nil {
		t.Fatalf("valid revision: %+v %v", patch, err)
	}
	var checkpoint graphtask.SessionCheckpointInput
	if err := decodeStrictTaskJSON(strings.NewReader(`{"summary":"Tests passed","problems":"No unresolved blocker","decisions":"Keep existing IDs","strategy":"Validate final scope","next_step":"Read associated tasks before closure"}`), "-", &checkpoint, "session checkpoint"); err != nil || checkpoint.NextStep == "" || checkpoint.Decisions == "" {
		t.Fatalf("valid checkpoint: %+v %v", checkpoint, err)
	}
	for _, payload := range []string{`{"summary":"one","next_step":"two","extra":true}`, `{"summary":"one"} {"next_step":"two"}`} {
		if err := decodeStrictTaskJSON(strings.NewReader(payload), "-", &graphtask.SessionCheckpointInput{}, "session checkpoint"); err == nil {
			t.Fatal("invalid checkpoint JSON accepted")
		}
	}
	if err := decodeStrictTaskJSON(strings.NewReader(`{"title":"one","session_id":"ses-elsewhere"}`), "-", &taskSessionRevisionPatch{}, "session revision"); err == nil {
		t.Fatal("unknown session revision field accepted")
	}
}
