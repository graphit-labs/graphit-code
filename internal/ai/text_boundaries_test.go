package ai

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func TestCompleteStream_MessageBoundaries(t *testing.T) {
	cases := []struct{ name, cli, input, want string }{
		{"codex preserves Markdown indentation and trailing newline", "codex", `
{"type":"item.completed","item":{"type":"agent_message","text":"    code block\n"}}
`, "    code block\n"},
		{"codex messages around a tool", "codex", `
{"type":"item.completed","item":{"id":"m1","type":"agent_message","text":"Vou consultar as fontes."}}
{"type":"item.started","item":{"id":"t1","type":"command_execution","command":"read"}}
{"type":"item.completed","item":{"id":"t1","type":"command_execution","aggregated_output":"ok"}}
{"type":"item.completed","item":{"id":"m2","type":"agent_message","text":"Estou usando **Knowledge** e [[context:Guide]]."}}
`, "Vou consultar as fontes.\n\nEstou usando **Knowledge** e [[context:Guide]]."},
		{"opencode complete parts", "opencode", `
{"type":"text","part":{"text":"First.","time":{"end":1}}}
{"type":"text","part":{"text":"Second.","time":{"end":2}}}
`, "First.\n\nSecond."},
		{"qwen keeps content blocks together", "qwen", `
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"**First"},{"type":"text","text":".**"}]}}
{"type":"assistant","message":{"role":"assistant","content":"Second."}}
`, "**First.**\n\nSecond."},
		{"kimi keeps content blocks together", "kimi", `
{"role":"assistant","content":[{"type":"text","text":"[First]"},{"type":"text","text":"(https://example.org)"}]}
{"role":"assistant","content":"Second."}
`, "[First](https://example.org)\n\nSecond."},
		{"claude native messages and deltas", "claude", `
{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"**First"}}}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":".**"}}}
{"type":"assistant","message":{"content":[{"type":"text","text":"**First.**"}]}}
{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"message_start"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool","name":"read","input":{}}]}}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"[Second]"}}}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"(https://example.org)"}}}
{"type":"assistant","message":{"content":[{"type":"text","text":"[Second](https://example.org)"}]}}
{"type":"stream_event","event":{"type":"message_start"}}
`, "**First.**\n\n[Second](https://example.org)"},
		{"gemini chunks with and without delta flag", "gemini", `
{"type":"message","role":"assistant","content":"**First","delta":true}
{"type":"message","role":"assistant","content":".**"}
`, "**First.**"},
		{"agy chunks", "agy", `
{"event":"step_update","step_update":{"text_delta":"[First]"}}
{"event":"step_update","step_update":{"text_delta":"(https://example.org)"}}
`, "[First](https://example.org)"},
		{"claude legacy flat deltas", "claude", `
{"type":"stream_event","delta":{"type":"text_delta","text":"**First"}}
{"type":"stream_event","delta":{"type":"text_delta","text":".**"}}
`, "**First.**"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			path := writeFakeCLI(t, tc.cli, "#!/bin/sh\ncat <<'EOF'\n"+tc.input+"\nEOF\n")
			client := &cliClient{executablePath: path, binaryName: tc.cli}
			// Each run owns its pending boundary; no separator leaks into another turn.
			for range 2 {
				var events []Event
				res, err := client.CompleteStream(context.Background(), StreamRequest{UserPrompt: "q"}, collect(&events))
				if err != nil {
					t.Fatal(err)
				}
				if res.Text != tc.want || textOf(events) != tc.want {
					t.Fatalf("result=%q events=%q want=%q", res.Text, textOf(events), tc.want)
				}
				for _, ev := range events {
					if ev.textStart || ev.Kind == EventText && ev.Text == "" {
						t.Fatalf("parser-only boundary escaped: %#v", ev)
					}
				}
			}
		})
	}
}

func TestMessageBoundaryPreservesExistingWhitespace(t *testing.T) {
	for _, parts := range [][2]string{{"First.", "Second."}, {"First.\n", "Second."}, {"First.\n", "\nSecond."}, {"First.\n\n", "Second."}, {"First.\r\n\r\n", "Second."}} {
		var lines []string
		for _, part := range parts {
			line, _ := json.Marshal(map[string]any{"type": "item.completed", "item": map[string]string{"type": "agent_message", "text": part}})
			lines = append(lines, string(line))
		}
		var result strings.Builder
		var events []Event
		var sid string
		readStructured(strings.NewReader(strings.Join(lines, "\n")), structuredStreams["codex"], collect(&events), &result, &sid)
		want := "First.\n\nSecond."
		if strings.Contains(parts[0], "\r") {
			want = "First.\r\n\r\nSecond."
		}
		if result.String() != want || textOf(events) != want {
			t.Fatalf("parts=%q result=%q events=%q", parts, result.String(), textOf(events))
		}
	}
	encoded, err := json.Marshal(Event{Kind: EventText, Text: "hello", textStart: true})
	if err != nil || strings.Contains(string(encoded), "Start") || strings.Contains(string(encoded), "start") {
		t.Fatalf("private marker serialized: %s (%v)", encoded, err)
	}
}
