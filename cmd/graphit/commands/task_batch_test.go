package commands

import (
	"strings"
	"testing"

	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func TestDecodeTaskBatchFromStdin(t *testing.T) {
	var input graphtask.BatchInput
	err := decodeTaskBatch(strings.NewReader(`{"lease":"2h","operations":[{"key":"one","action":"create","title":"First"},{"key":"two","action":"cancel","id":"tsk-abcd","reason":"obsolete"}]}`), "-", &input)
	if err != nil {
		t.Fatal(err)
	}
	if input.Lease != "2h" || len(input.Operations) != 2 || input.Operations[0].Key != "one" || input.Operations[1].Action != "cancel" {
		t.Fatalf("decoded batch = %#v", input)
	}
}

func TestDecodeTaskBatchRejectsUnknownFieldsAndMultipleValues(t *testing.T) {
	for name, payload := range map[string]string{
		"unknown":  `{"operations":[],"surprise":true}`,
		"multiple": `{"operations":[]} {"operations":[]}`,
	} {
		t.Run(name, func(t *testing.T) {
			var input graphtask.BatchInput
			if err := decodeTaskBatch(strings.NewReader(payload), "-", &input); err == nil {
				t.Fatal("invalid batch JSON was accepted")
			}
		})
	}
}

func TestDecodeTaskRevisionPatchStrictly(t *testing.T) {
	var patch taskRevisionPatch
	err := decodeStrictTaskJSON(strings.NewReader(`{"description":"Corrected specification","depends_on":[],"add_tests":["Focused test"]}`), "-", &patch, "task revision patch")
	if err != nil {
		t.Fatal(err)
	}
	if patch.Description == nil || *patch.Description != "Corrected specification" || patch.DependsOn == nil || len(*patch.DependsOn) != 0 || len(patch.AddTests) != 1 {
		t.Fatalf("decoded patch = %#v", patch)
	}
	if err := decodeStrictTaskJSON(strings.NewReader(`{"unknown":true}`), "-", &taskRevisionPatch{}, "task revision patch"); err == nil {
		t.Fatal("unknown revision patch field was accepted")
	}
}

func TestTaskRevisionCommandsAreReachable(t *testing.T) {
	root := newTaskCmd()
	if command, _, err := root.Find([]string{"revise"}); err != nil || command == root {
		t.Fatalf("task revise command not found: %v", err)
	}
	if command, _, err := root.Find([]string{"check", "supersede"}); err != nil || command.Name() != "supersede" {
		t.Fatalf("task check supersede command not found: %v", err)
	}
}
