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
