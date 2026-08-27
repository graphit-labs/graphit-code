package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// writeFakeCLI drops an executable shell script and returns its path.
//
// Skips on Windows rather than failing: the script is /bin/sh, and the surrounding
// package already tests this way. The parsing tests below are platform-independent and
// carry the logic that could actually be wrong.
func writeFakeCLI(t *testing.T, name, script string) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("fake CLI is a shell script")
	}
	path := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
		t.Fatalf("writing fake %s: %v", name, err)
	}
	return path
}

func collect(events *[]Event) EventFunc {
	return func(ev Event) { *events = append(*events, ev) }
}

func kinds(events []Event) []EventKind {
	out := make([]EventKind, len(events))
	for i, ev := range events {
		out[i] = ev.Kind
	}
	return out
}

func textOf(events []Event) string {
	var b strings.Builder
	for _, ev := range events {
		if ev.Kind == EventText {
			b.WriteString(ev.Text)
		}
	}
	return b.String()
}

// Every CLI in the table must stream, because every one of them writes its answer to
// stdout. This is the "coverage by construction" claim, tested rather than asserted in
// a comment: if someone adds a CLI to knownCLIs, it is covered here the same day.
func TestCompleteStream_CoversEveryKnownCLI(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("fake CLI is a shell script")
	}
	dir := t.TempDir()

	var names []string
	for name := range knownCLIs {
		names = append(names, name)
	}
	// Plus a binary the table does not know, which must fall through to the generic
	// spec rather than being excluded.
	names = append(names, "some-unknown-cli")

	for _, name := range names {
		want := name + " answered"

		// A CLI with a structured mode is asked for JSON, so the fake must speak it.
		// The plain-text fallback for such a CLI is covered separately below.
		var script string
		if _, structured := streamSpecFor(name); structured {
			line := fmt.Sprintf(
				`{"type":"stream_event","delta":{"type":"text_delta","text":%q}}`, want)
			script = "#!/bin/sh\ncat <<'EOF'\n" + line + "\nEOF\n"
		} else {
			script = fmt.Sprintf("#!/bin/sh\nprintf '%s'\n", want)
		}

		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
			t.Fatalf("writing fake %s: %v", name, err)
		}

		t.Run(name, func(t *testing.T) {
			c := &cliClient{executablePath: path, binaryName: name}
			var events []Event
			res, err := c.CompleteStream(context.Background(),
				StreamRequest{UserPrompt: "q"}, collect(&events))
			if err != nil {
				t.Fatalf("CompleteStream: %v", err)
			}
			if res.Text != want {
				t.Errorf("Text = %q, want %q", res.Text, want)
			}
			if got := textOf(events); got != want {
				t.Errorf("text events = %q, want %q", got, want)
			}
			if len(events) == 0 || events[len(events)-1].Kind != EventDone {
				t.Errorf("last event = %v, want done", kinds(events))
			}
		})
	}
}

// A CLI whose version predates the structured flag prints ordinary text. That used to
// produce an EMPTY answer with no error — the worst available outcome, because it looks
// like the model had nothing to say.
func TestCompleteStream_StructuredCLIFallsBackToText(t *testing.T) {
	path := writeFakeCLI(t, "claude",
		"#!/bin/sh\necho 'plain prose, no json here'\necho 'second line'\n")

	c := &cliClient{executablePath: path, binaryName: "claude"}
	var events []Event
	res, err := c.CompleteStream(context.Background(),
		StreamRequest{UserPrompt: "q"}, collect(&events))
	if err != nil {
		t.Fatalf("CompleteStream: %v", err)
	}

	for _, want := range []string{"plain prose, no json here", "second line"} {
		if !strings.Contains(res.Text, want) {
			t.Errorf("Text missing %q, got %q", want, res.Text)
		}
	}
	// And it must say the detail was not observable, rather than claim otherwise.
	if res.Structured {
		t.Error("Structured must be false when the stream was not the structured format")
	}
	if textOf(events) == "" {
		t.Error("the fallback text must also reach the caller as events")
	}
}

// A reader waiting on a long final paragraph is the case incremental delivery exists
// for, so the text must arrive in pieces rather than in one lump at exit. Forced with
// a payload larger than the read buffer, which makes it deterministic instead of
// timing-dependent.
func TestCompleteStream_TextArrivesIncrementally(t *testing.T) {
	path := writeFakeCLI(t, "gemini",
		"#!/bin/sh\nfor i in $(seq 1 400); do printf '0123456789012345678901234567890123456789'; done\n")

	c := &cliClient{executablePath: path, binaryName: "gemini"}
	var events []Event
	res, err := c.CompleteStream(context.Background(),
		StreamRequest{UserPrompt: "q"}, collect(&events))
	if err != nil {
		t.Fatalf("CompleteStream: %v", err)
	}

	if len(res.Text) != 16000 {
		t.Fatalf("Text length = %d, want 16000", len(res.Text))
	}
	var textEvents int
	for _, ev := range events {
		if ev.Kind == EventText {
			textEvents++
		}
	}
	if textEvents < 2 {
		t.Errorf("got %d text events for a 16 KB answer, want it split", textEvents)
	}
	if got := textOf(events); got != res.Text {
		t.Error("concatenated text events must equal the result text")
	}
}

// The working directory is the whole reason the live search can rely on files written
// by init: an agent CLI reads its rules, skills and MCP servers from where it runs.
func TestCompleteStream_RunsInWorkDir(t *testing.T) {
	path := writeFakeCLI(t, "claude-plain", "#!/bin/sh\npwd\n")
	workDir := t.TempDir()

	c := &cliClient{executablePath: path, binaryName: "claude-plain"}
	res, err := c.CompleteStream(context.Background(),
		StreamRequest{UserPrompt: "q", WorkDir: workDir}, nil)
	if err != nil {
		t.Fatalf("CompleteStream: %v", err)
	}

	// The temp dir may be reached through a symlink (/tmp on macOS), so compare the
	// resolved forms.
	want, err := filepath.EvalSymlinks(workDir)
	if err != nil {
		t.Fatal(err)
	}
	got, err := filepath.EvalSymlinks(strings.TrimSpace(res.Text))
	if err != nil {
		t.Fatalf("resolving reported cwd %q: %v", res.Text, err)
	}
	if got != want {
		t.Errorf("ran in %q, want %q", got, want)
	}
}

func TestCompleteStream_FailureStillEndsWithDone(t *testing.T) {
	path := writeFakeCLI(t, "grok", "#!/bin/sh\necho 'something broke' >&2\nexit 3\n")

	c := &cliClient{executablePath: path, binaryName: "grok"}
	var events []Event
	_, err := c.CompleteStream(context.Background(),
		StreamRequest{UserPrompt: "q"}, collect(&events))
	if err == nil {
		t.Fatal("expected an error from a CLI that exited 3")
	}
	// stderr is where a failing CLI explains itself, so it must reach the error
	// rather than being discarded.
	if !strings.Contains(err.Error(), "something broke") {
		t.Errorf("error must carry stderr, got: %v", err)
	}

	k := kinds(events)
	if len(k) < 2 || k[len(k)-1] != EventDone || k[len(k)-2] != EventError {
		t.Errorf("events = %v, want …error, done", k)
	}
}

func TestCompleteStream_ForwardsStderr(t *testing.T) {
	path := writeFakeCLI(t, "codex", "#!/bin/sh\necho 'warming up' >&2\nprintf 'answer'\n")

	c := &cliClient{executablePath: path, binaryName: "codex"}
	var events []Event
	if _, err := c.CompleteStream(context.Background(),
		StreamRequest{UserPrompt: "q"}, collect(&events)); err != nil {
		t.Fatalf("CompleteStream: %v", err)
	}

	var sawStderr bool
	for _, ev := range events {
		if ev.Kind == EventStderr && strings.Contains(ev.Text, "warming up") {
			sawStderr = true
		}
	}
	if !sawStderr {
		t.Errorf("expected stderr forwarded as an event, got %v", kinds(events))
	}
}

// End-to-end for the structured path: tool activity becomes events and the session ID
// is learned from the CLI rather than echoed back.
func TestCompleteStream_StructuredClaude(t *testing.T) {
	lines := []string{
		`{"type":"system","subtype":"init","session_id":"sess-abc"}`,
		`{"type":"stream_event","delta":{"type":"text_delta","text":"Looking"}}`,
		`{"type":"assistant","message":{"content":[{"type":"tool_use","name":"graphit_ast_query","input":{"query":"MATCH (n) RETURN n"}}]}}`,
		`{"type":"user","message":{"content":[{"type":"tool_result","content":"3 rows"}]}}`,
		`{"type":"stream_event","delta":{"type":"text_delta","text":" done."}}`,
		`{"type":"an_event_type_from_a_future_release","payload":{"x":1}}`,
		`not json at all`,
		`{"type":"result","subtype":"success","result":"Looking done."}`,
	}
	script := "#!/bin/sh\ncat <<'EOF'\n" + strings.Join(lines, "\n") + "\nEOF\n"
	path := writeFakeCLI(t, "claude", script)

	c := &cliClient{executablePath: path, binaryName: "claude"}
	if !c.SupportsStructuredStream() {
		t.Fatal("claude must report structured stream support")
	}

	var events []Event
	res, err := c.CompleteStream(context.Background(),
		StreamRequest{UserPrompt: "q"}, collect(&events))
	if err != nil {
		t.Fatalf("CompleteStream: %v", err)
	}

	if res.Text != "Looking done." {
		t.Errorf("Text = %q, want %q", res.Text, "Looking done.")
	}
	// Learned, not echoed. The old implementation returned whatever ID it was given,
	// so a first turn could never produce one and --resume had nothing to resume.
	if res.SessionID != "sess-abc" {
		t.Errorf("SessionID = %q, want sess-abc", res.SessionID)
	}
	if !res.Structured {
		t.Error("expected Structured = true")
	}

	var toolUse, toolResult int
	for _, ev := range events {
		switch ev.Kind {
		case EventToolUse:
			toolUse++
			if ev.Tool != "graphit_ast_query" {
				t.Errorf("tool = %q, want graphit_ast_query", ev.Tool)
			}
			if !strings.Contains(ev.Detail, "MATCH (n) RETURN n") {
				t.Errorf("tool input missing from detail: %q", ev.Detail)
			}
		case EventToolResult:
			toolResult++
			if ev.Detail != "3 rows" {
				t.Errorf("tool result = %q, want %q", ev.Detail, "3 rows")
			}
		}
	}
	if toolUse != 1 || toolResult != 1 {
		t.Errorf("tool events = %d use / %d result, want 1 / 1", toolUse, toolResult)
	}
	if last := events[len(events)-1].Kind; last != EventDone {
		t.Errorf("last event = %q, want done", last)
	}
}

func TestSupportsStructuredStream(t *testing.T) {
	t.Parallel()
	if !(&cliClient{binaryName: "claude"}).SupportsStructuredStream() {
		t.Error("claude has a structured mode")
	}
	// The rest stream as plain text, which is not a gap — it is the universal path.
	for _, bin := range []string{"gemini", "codex", "opencode", "goose", "unknown"} {
		if (&cliClient{binaryName: bin}).SupportsStructuredStream() {
			t.Errorf("%s must not claim a structured mode it has no parser for", bin)
		}
	}
}

func TestParseClaudeStreamLine(t *testing.T) {
	t.Parallel()

	t.Run("init carries the session id", func(t *testing.T) {
		_, _, sid := parseClaudeStreamLine([]byte(`{"type":"system","subtype":"init","session_id":"s1"}`))
		if sid != "s1" {
			t.Errorf("sessionID = %q, want s1", sid)
		}
	})

	t.Run("text delta becomes text", func(t *testing.T) {
		evs, text, _ := parseClaudeStreamLine(
			[]byte(`{"type":"stream_event","delta":{"type":"text_delta","text":"hi"}}`))
		if text != "hi" || len(evs) != 1 || evs[0].Kind != EventText {
			t.Errorf("got %v / %q", kinds(evs), text)
		}
	})

	t.Run("whole-message text is not double counted", func(t *testing.T) {
		// The deltas already carried it; accumulating both duplicates the answer.
		evs, text, _ := parseClaudeStreamLine(
			[]byte(`{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}`))
		if text != "" || len(evs) != 0 {
			t.Errorf("expected nothing accumulated, got %v / %q", kinds(evs), text)
		}
	})

	t.Run("thinking is separate from the answer", func(t *testing.T) {
		evs, text, _ := parseClaudeStreamLine(
			[]byte(`{"type":"assistant","message":{"content":[{"type":"thinking","text":"hmm"}]}}`))
		if text != "" {
			t.Errorf("thinking must not enter the answer, got %q", text)
		}
		if len(evs) != 1 || evs[0].Kind != EventThinking {
			t.Errorf("got %v", kinds(evs))
		}
	})

	t.Run("failed result becomes an error", func(t *testing.T) {
		evs, _, _ := parseClaudeStreamLine(
			[]byte(`{"type":"result","subtype":"error_max_turns","result":"gave up"}`))
		if len(evs) != 1 || evs[0].Kind != EventError || evs[0].Text != "gave up" {
			t.Errorf("got %v", evs)
		}
	})

	t.Run("unknown and malformed lines are ignored", func(t *testing.T) {
		// These formats gain event types between releases. Erroring on an
		// unrecognised line turns somebody else's minor bump into an outage.
		for _, line := range []string{
			`{"type":"brand_new_event"}`,
			`{"malformed`,
			`[]`,
			``,
		} {
			evs, text, sid := parseClaudeStreamLine([]byte(line))
			if len(evs) != 0 || text != "" || sid != "" {
				t.Errorf("line %q produced %v / %q / %q", line, kinds(evs), text, sid)
			}
		}
	})
}

func TestRenderToolPayload(t *testing.T) {
	t.Parallel()

	// A JSON string renders without quotes and escapes; anything else keeps its shape.
	if got := renderToolPayload(json.RawMessage(`"3 rows"`)); got != "3 rows" {
		t.Errorf("string payload = %q, want 3 rows", got)
	}
	if got := renderToolPayload(json.RawMessage(`{"q":1}`)); got != `{"q":1}` {
		t.Errorf("object payload = %q", got)
	}
	if got := renderToolPayload(nil); got != "" {
		t.Errorf("empty payload = %q", got)
	}

	long := json.RawMessage(`"` + strings.Repeat("x", 3000) + `"`)
	got := renderToolPayload(long)
	if len(got) > 2100 {
		t.Errorf("payload not truncated: %d chars", len(got))
	}
	// Truncation must announce itself; a silently cut tool result reads as complete.
	if !strings.Contains(got, "truncated") {
		t.Error("truncation must be stated")
	}
}

// The two preambles are the sandbox decision, so the difference is asserted rather
// than left to inspection: one forbids tool use, the other requires it.
func TestPreambleFor(t *testing.T) {
	t.Parallel()

	restricted := preambleFor(false)
	if !strings.Contains(restricted, "Do NOT execute actions that require user approval") {
		t.Error("the default preamble must keep forbidding tool use")
	}

	agentic := preambleFor(true)
	if strings.Contains(agentic, "Do NOT execute actions that require user approval") {
		t.Error("the agentic preamble must not forbid the tools it exists to allow")
	}
	for _, want := range []string{"DO use your available tools", "Do NOT ask the user any questions"} {
		if !strings.Contains(agentic, want) {
			t.Errorf("agentic preamble missing %q", want)
		}
	}
}
