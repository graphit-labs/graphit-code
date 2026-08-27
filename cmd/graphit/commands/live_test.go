package commands

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
)

func TestParseArtifactSpecAcceptsEveryFormAndRefusesTheRest(t *testing.T) {
	good := []struct {
		spec    string
		id      string
		artType string
		version string
	}{
		{"acme-docs", "acme-docs", "", ""},
		{"knowledge:acme-docs", "acme-docs", "knowledge", ""},
		{"acme-docs@2.1.0", "acme-docs", "", "2.1.0"},
		{"ast:acme-graph@1.0.0", "acme-graph", "ast", "1.0.0"},
		{"  acme-docs  ", "acme-docs", "", ""},
		{"team/acme-docs", "team/acme-docs", "", ""},
	}
	for _, c := range good {
		got, err := parseArtifactSpec(c.spec)
		if err != nil {
			t.Fatalf("parseArtifactSpec(%q): %v", c.spec, err)
		}
		if got.ID != c.id || got.Type != c.artType || got.Version != c.version {
			t.Fatalf("parseArtifactSpec(%q) = %+v, want id=%q type=%q version=%q",
				c.spec, got, c.id, c.artType, c.version)
		}
	}

	bad := []string{
		"",             // nothing chosen
		"   ",          // nothing chosen
		"banana:thing", // no such artifact type
		"knowledge:",   // a type and no ID
		"../etc/passwd",
		"/absolute",
		"has spaces",
	}
	for _, spec := range bad {
		if _, err := parseArtifactSpec(spec); err == nil {
			t.Fatalf("parseArtifactSpec(%q) was accepted", spec)
		}
	}
}

func TestParseArtifactSpecSplitsTheVersionTheWayTheInstallerWill(t *testing.T) {
	// An artifact ID may legally contain "@", and hub.Install splits on the first
	// one. Agreeing with it matters more than being theoretically right: the
	// artifact this command reports must be the artifact that gets installed.
	got, err := parseArtifactSpec("scope@name@3.0.0")
	if err != nil {
		t.Fatalf("parseArtifactSpec: %v", err)
	}
	if got.ID != "scope" || got.Version != "name@3.0.0" {
		t.Fatalf("got id=%q version=%q, want the first @ to separate them", got.ID, got.Version)
	}
}

func TestParseArtifactSpecsStopsAtTheFirstBadOne(t *testing.T) {
	if _, err := parseArtifactSpecs([]string{"fine", "banana:bad", "also-fine"}); err == nil {
		t.Fatal("a bad artifact in the middle was accepted")
	}
	got, err := parseArtifactSpecs([]string{"one", "two@1.0"})
	if err != nil {
		t.Fatalf("parseArtifactSpecs: %v", err)
	}
	if len(got) != 2 || got[0].ID != "one" || got[1].Version != "1.0" {
		t.Fatalf("unexpected result: %+v", got)
	}
}

func TestTheAnswerIsWrittenRawSoChunksFormOneText(t *testing.T) {
	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)

	for _, chunk := range []string{"The parser ", "lives in ", "internal/ast."} {
		r.render(livesearch.Event{Kind: livesearch.KindText, Text: chunk})
	}
	if got := buf.String(); got != "The parser lives in internal/ast." {
		// A prefix or a newline per chunk would land inside a sentence.
		t.Fatalf("the answer came out as %q", got)
	}
}

func TestAMessageDuringTheAnswerDoesNotCutASentenceInHalf(t *testing.T) {
	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)

	r.render(livesearch.Event{Kind: livesearch.KindText, Text: "before"})
	r.render(livesearch.Event{Kind: livesearch.KindToolUse, Tool: "graphit_ast_query", Detail: "MATCH (n)"})
	r.render(livesearch.Event{Kind: livesearch.KindText, Text: "after"})

	out := buf.String()
	lines := strings.Split(out, "\n")
	if lines[0] != "before" {
		t.Fatalf("the answer so far should have been closed on its own line, got %q", lines[0])
	}
	if !strings.Contains(out, "graphit_ast_query") {
		t.Fatalf("the tool call was not reported: %q", out)
	}
	if !strings.Contains(out, "after") {
		t.Fatalf("the answer did not resume: %q", out)
	}
	if strings.Contains(out, "beforegraphit_ast_query") {
		t.Fatalf("the message was written inline with the answer: %q", out)
	}
}

func TestQuietByDefaultAndDetailedWhenAsked(t *testing.T) {
	noisy := []livesearch.Event{
		{Kind: livesearch.KindThinking, Text: "let me look"},
		{Kind: livesearch.KindToolResult, Tool: "graphit_ast_query", Detail: "500 rows"},
		{Kind: livesearch.KindStderr, Text: "a warning from the cli"},
	}

	var quiet bytes.Buffer
	rq := newLiveRenderer(&quiet, false, false)
	for _, ev := range noisy {
		rq.render(ev)
	}
	if quiet.Len() != 0 {
		t.Fatalf("the default output carried diagnostics: %q", quiet.String())
	}

	var loud bytes.Buffer
	rl := newLiveRenderer(&loud, false, true)
	for _, ev := range noisy {
		rl.render(ev)
	}
	for _, want := range []string{"let me look", "500 rows", "a warning from the cli"} {
		if !strings.Contains(loud.String(), want) {
			t.Fatalf("--verbose did not show %q: %q", want, loud.String())
		}
	}
}

func TestPreparationProgressAndErrorsAreAlwaysShown(t *testing.T) {
	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)

	r.render(livesearch.Event{Kind: livesearch.KindPrep, Text: "compiling the wiki"})
	r.render(livesearch.Event{Kind: livesearch.KindError, Text: "the hub was unreachable"})
	r.render(livesearch.Event{Kind: livesearch.KindPrompt, Text: "why is it slow"})

	out := buf.String()
	for _, want := range []string{"compiling the wiki", "the hub was unreachable", "why is it slow"} {
		if !strings.Contains(out, want) {
			t.Fatalf("%q is missing from the output: %q", want, out)
		}
	}
}

func TestTurnDoneIsReportedToTheCaller(t *testing.T) {
	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)

	if r.render(livesearch.Event{Kind: livesearch.KindText, Text: "x"}) {
		t.Fatal("a text chunk must not end the turn")
	}
	if !r.render(livesearch.Event{Kind: livesearch.KindTurnDone}) {
		t.Fatal("turn_done must end the turn")
	}
}

func TestJSONModeEmitsTheRecordedEventOnePerLine(t *testing.T) {
	// A script downstream should see what the log holds, not a summary of it.
	var buf bytes.Buffer
	r := newLiveRenderer(&buf, true, false)

	events := []livesearch.Event{
		{Seq: 1, Kind: livesearch.KindPrep, Text: "installing"},
		{Seq: 2, Kind: livesearch.KindText, Text: "line one\nline two"},
		{Seq: 3, Kind: livesearch.KindTurnDone},
	}
	for _, ev := range events {
		r.render(ev)
	}

	lines := strings.Split(strings.TrimRight(buf.String(), "\n"), "\n")
	if len(lines) != 3 {
		t.Fatalf("got %d lines for 3 events: %q", len(lines), buf.String())
	}
	var second livesearch.Event
	if err := json.Unmarshal([]byte(lines[1]), &second); err != nil {
		t.Fatalf("line 2 is not JSON: %v", err)
	}
	// The embedded newline must have survived as data rather than splitting a line.
	if second.Text != "line one\nline two" || second.Seq != 2 {
		t.Fatalf("the event was altered: %+v", second)
	}
}

// fakeLiveClient is an ai.StreamClient the live CLI tests drive.
type fakeLiveClient struct {
	stream func(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error)
}

func (f *fakeLiveClient) Complete(context.Context, string, string) (string, error) {
	return "", errors.New("not used")
}

func (f *fakeLiveClient) SupportsStructuredStream() bool { return true }

func (f *fakeLiveClient) CompleteStream(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	if f.stream == nil {
		return &ai.StreamResult{}, nil
	}
	return f.stream(ctx, req, emit)
}

func newLiveTestSession(t *testing.T, client ai.StreamClient) *livesearch.Session {
	t.Helper()
	mgr := livesearch.NewManager(t.TempDir(), client, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	deadline := time.Now().Add(5 * time.Second)
	for s.State() != livesearch.StateReady && time.Now().Before(deadline) {
		time.Sleep(2 * time.Millisecond)
	}
	if s.State() != livesearch.StateReady {
		t.Fatalf("session state is %q, want ready", s.State())
	}
	return s
}

func TestTheConversationAsksAgainUntilToldToLeave(t *testing.T) {
	client := &fakeLiveClient{stream: func(_ context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		emit(ai.Event{Kind: ai.EventText, Text: "answer to " + req.UserPrompt})
		return &ai.StreamResult{}, nil
	}}
	s := newLiveTestSession(t, client)

	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)
	events, stop := s.Subscribe(0)
	defer stop()

	in := strings.NewReader("first question\n\nsecond question\n/exit\n")
	if err := r.converse(context.Background(), s, events, make(chan os.Signal), in); err != nil {
		t.Fatalf("converse: %v", err)
	}

	out := buf.String()
	for _, want := range []string{"answer to first question", "answer to second question"} {
		if !strings.Contains(out, want) {
			t.Fatalf("%q is missing: %q", want, out)
		}
	}
	// The blank line between them must not have been sent as a question.
	if strings.Contains(out, "answer to \n") {
		t.Fatalf("an empty question was sent: %q", out)
	}
}

func TestEndOfInputLeavesTheConversationWithoutAnError(t *testing.T) {
	s := newLiveTestSession(t, &fakeLiveClient{})
	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)
	events, stop := s.Subscribe(0)
	defer stop()

	// No trailing newline and no /exit: a closed pipe, or Ctrl-D.
	if err := r.converse(context.Background(), s, events, make(chan os.Signal), strings.NewReader("")); err != nil {
		t.Fatalf("converse: %v", err)
	}
}

func TestInterruptingStopsTheAnswerAndKeepsTheSession(t *testing.T) {
	// Ctrl-C during an answer must cancel the turn, not the session — and must keep
	// reading, because cancelling is what produces the events that say so.
	release := make(chan struct{})
	client := &fakeLiveClient{stream: func(ctx context.Context, _ ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		emit(ai.Event{Kind: ai.EventText, Text: "starting"})
		close(release)
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	s := newLiveTestSession(t, client)

	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)
	events, stop := s.Subscribe(0)
	defer stop()

	if err := s.Send("a long one"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	<-release

	interrupts := make(chan os.Signal, 1)
	interrupts <- os.Interrupt

	if err := r.streamTurn(context.Background(), s, events, interrupts); err != nil {
		t.Fatalf("streamTurn: %v", err)
	}

	out := buf.String()
	if !strings.Contains(out, "stopping this answer") {
		t.Fatalf("the interrupt was not acknowledged: %q", out)
	}
	if !strings.Contains(out, "cancelled") {
		t.Fatalf("the cancellation was not reported: %q", out)
	}
	// The session survived and can take another question.
	deadline := time.Now().Add(5 * time.Second)
	for s.State() != livesearch.StateReady && time.Now().Before(deadline) {
		time.Sleep(2 * time.Millisecond)
	}
	if s.State() != livesearch.StateReady {
		t.Fatalf("the session is %q after an interrupt, want ready", s.State())
	}
}

func TestASecondInterruptLeaves(t *testing.T) {
	client := &fakeLiveClient{stream: func(ctx context.Context, _ ai.StreamRequest, _ ai.EventFunc) (*ai.StreamResult, error) {
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	s := newLiveTestSession(t, client)

	var buf bytes.Buffer
	r := newLiveRenderer(&buf, false, false)
	events, stop := s.Subscribe(0)
	defer stop()

	if err := s.Send("a long one"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	interrupts := make(chan os.Signal, 2)
	interrupts <- os.Interrupt
	interrupts <- os.Interrupt

	err := r.streamTurn(context.Background(), s, events, interrupts)
	if !errors.Is(err, errInterrupted) {
		t.Fatalf("streamTurn returned %v, want errInterrupted", err)
	}
}

func TestLineReaderSplitsInputTheWayATerminalDoes(t *testing.T) {
	// Including the last line when the input ends without a newline, which is what a
	// pipe from echo -n produces.
	lr := newLineReader(strings.NewReader("one\r\ntwo\nthree"))
	defer lr.stop()

	var got []string
	for line := range lr.lines {
		got = append(got, line)
	}
	if len(got) != 3 || got[0] != "one" || got[1] != "two" || got[2] != "three" {
		t.Fatalf("got %q, want [one two three]", got)
	}
}

func TestLineReaderStopsWithoutLeakingItsGoroutine(t *testing.T) {
	// A reader that never ends: stopping must return rather than wait for input that
	// is not coming.
	pr, pw := io.Pipe()
	t.Cleanup(func() { _ = pw.Close() })

	lr := newLineReader(pr)
	go func() { _, _ = pw.Write([]byte("hello\n")) }()

	select {
	case got := <-lr.lines:
		if got != "hello" {
			t.Fatalf("got %q", got)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the line never arrived")
	}
	lr.stop()
}

func TestLiveCommandRefusesToSearchNothing(t *testing.T) {
	err := runLive(context.Background(), liveOptions{question: "anything"})
	if err == nil {
		t.Fatal("a search with no artifacts was accepted")
	}
	if !strings.Contains(err.Error(), "at least one artifact") {
		t.Fatalf("unhelpful error: %v", err)
	}
}

func TestLiveCommandRefusesAnUnsupportedIDEBeforeDoingAnyWork(t *testing.T) {
	err := runLive(context.Background(), liveOptions{
		question:  "anything",
		artifacts: []string{"acme-docs"},
		ide:       "vscode",
	})
	if err == nil {
		t.Fatal("an unsupported IDE was accepted")
	}
	if !strings.Contains(err.Error(), "unsupported IDE") {
		t.Fatalf("unhelpful error: %v", err)
	}
}

func TestLiveCommandRejectsABadArtifactBeforeTouchingTheHub(t *testing.T) {
	err := runLive(context.Background(), liveOptions{
		question:  "anything",
		artifacts: []string{"../etc/passwd"},
	})
	if err == nil {
		t.Fatal("a traversal was accepted")
	}
	if !strings.Contains(err.Error(), "path traversal") {
		t.Fatalf("unhelpful error: %v", err)
	}
}

func TestFirstLineKeepsOutputToOneLine(t *testing.T) {
	if got := firstLine("  one\ntwo  ", 100); got != "one" {
		t.Fatalf("firstLine = %q, want one", got)
	}
	if got := firstLine(strings.Repeat("x", 20), 5); got != "xxxxx…" {
		t.Fatalf("firstLine = %q", got)
	}
	if got := firstLine("", 5); got != "" {
		t.Fatalf("firstLine = %q, want empty", got)
	}
}

func TestArtifactTypeListNamesEveryHubType(t *testing.T) {
	got := artifactTypeList()
	for _, want := range []string{"knowledge", "ast", "rule", "skill", "command", "agent", "mcp", "language"} {
		if !strings.Contains(got, want) {
			t.Fatalf("%q is missing from the advertised types: %q", want, got)
		}
	}
}
