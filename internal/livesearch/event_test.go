package livesearch

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

func openTestLog(t *testing.T) *eventLog {
	t.Helper()
	l, err := openEventLog(filepath.Join(t.TempDir(), eventsFileName))
	if err != nil {
		t.Fatalf("openEventLog: %v", err)
	}
	t.Cleanup(func() { _ = l.close() })
	return l
}

func collect(t *testing.T, l *eventLog, after, upto int64) []Event {
	t.Helper()
	var got []Event
	if err := l.replay(after, upto, func(ev Event) error {
		got = append(got, ev)
		return nil
	}); err != nil {
		t.Fatalf("replay: %v", err)
	}
	return got
}

func TestAppendNumbersEventsFromOne(t *testing.T) {
	l := openTestLog(t)
	for i := 0; i < 3; i++ {
		ev, err := l.append(Event{Kind: KindText, Text: "chunk"})
		if err != nil {
			t.Fatalf("append: %v", err)
		}
		if want := int64(i + 1); ev.Seq != want {
			t.Fatalf("event %d got seq %d, want %d", i, ev.Seq, want)
		}
	}
	if l.lastSeq() != 3 {
		t.Fatalf("lastSeq is %d, want 3", l.lastSeq())
	}
}

func TestAppendStampsATimeWhenTheCallerDidNot(t *testing.T) {
	l := openTestLog(t)
	ev, err := l.append(Event{Kind: KindPrep, Text: "indexing"})
	if err != nil {
		t.Fatalf("append: %v", err)
	}
	if ev.At.IsZero() {
		t.Fatal("an event with no timestamp is an event with no place in the transcript")
	}
}

func TestRecoveryTrustsTheRecordedSequenceNotTheLineCount(t *testing.T) {
	// A process killed mid-write leaves a partial final line. Counting lines would
	// hand the next event a number that is already taken, and two events sharing an
	// SSE id means a reconnecting client silently loses one of them.
	path := filepath.Join(t.TempDir(), eventsFileName)
	body := `{"seq":1,"kind":"text","text":"a","at":"2024-01-01T00:00:00Z"}
{"seq":2,"kind":"text","text":"b","at":"2024-01-01T00:00:00Z"}
{"seq":3,"kind":"text","text":"c","at":"2024-01-01T00:00:00Z"}
{"seq":4,"kind":"text","te`
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}

	l, err := openEventLog(path)
	if err != nil {
		t.Fatalf("openEventLog: %v", err)
	}
	defer func() { _ = l.close() }()

	if l.lastSeq() != 3 {
		t.Fatalf("recovered lastSeq is %d, want 3 — the truncated record must not count", l.lastSeq())
	}
	ev, err := l.append(Event{Kind: KindText, Text: "d"})
	if err != nil {
		t.Fatalf("append: %v", err)
	}
	if ev.Seq != 4 {
		t.Fatalf("the event after recovery got seq %d, want 4", ev.Seq)
	}

	// The truncated line must not have swallowed the new one.
	got := collect(t, l, 0, 0)
	if len(got) != 4 {
		t.Fatalf("replay yielded %d events, want 4: %+v", len(got), got)
	}
	if got[3].Seq != 4 || got[3].Text != "d" {
		t.Fatalf("the appended event was corrupted by the truncated line: %+v", got[3])
	}
}

func TestReplayIsBoundedAtBothEnds(t *testing.T) {
	l := openTestLog(t)
	for i := 0; i < 5; i++ {
		if _, err := l.append(Event{Kind: KindText}); err != nil {
			t.Fatalf("append: %v", err)
		}
	}

	got := collect(t, l, 2, 4)
	if len(got) != 2 || got[0].Seq != 3 || got[1].Seq != 4 {
		t.Fatalf("replay(2,4) yielded %+v, want events 3 and 4", got)
	}

	// Zero upto means everything after the lower bound, which is what a CLI dumping
	// a whole session needs.
	if got := collect(t, l, 0, 0); len(got) != 5 {
		t.Fatalf("unbounded replay yielded %d events, want 5", len(got))
	}
	if got := collect(t, l, 5, 0); len(got) != 0 {
		t.Fatalf("replay past the end yielded %d events, want none", len(got))
	}
}

func TestReplayStopsEarlyOnRequestWithoutReportingFailure(t *testing.T) {
	l := openTestLog(t)
	for i := 0; i < 10; i++ {
		if _, err := l.append(Event{Kind: KindText}); err != nil {
			t.Fatalf("append: %v", err)
		}
	}

	var seen int
	err := l.replay(0, 0, func(Event) error {
		seen++
		if seen == 3 {
			return errStopReplay
		}
		return nil
	})
	if err != nil {
		t.Fatalf("stopping a replay is not a failure, got %v", err)
	}
	if seen != 3 {
		t.Fatalf("replay delivered %d events after being told to stop at 3", seen)
	}
}

func TestReplayPropagatesARealCallerError(t *testing.T) {
	l := openTestLog(t)
	if _, err := l.append(Event{Kind: KindText}); err != nil {
		t.Fatalf("append: %v", err)
	}
	sentinel := errors.New("the subscriber went away")
	if err := l.replay(0, 0, func(Event) error { return sentinel }); !errors.Is(err, sentinel) {
		t.Fatalf("replay returned %v, want the caller's error", err)
	}
}

func TestReplaySkipsCorruptLinesAndKeepsTheRest(t *testing.T) {
	path := filepath.Join(t.TempDir(), eventsFileName)
	body := `{"seq":1,"kind":"text","text":"first"}
not json at all
{"seq":3,"kind":"text","text":"third"}
`
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
	l, err := openEventLog(path)
	if err != nil {
		t.Fatalf("openEventLog: %v", err)
	}
	defer func() { _ = l.close() }()

	got := collect(t, l, 0, 0)
	if len(got) != 2 || got[0].Text != "first" || got[1].Text != "third" {
		t.Fatalf("one bad line cost more than itself: %+v", got)
	}
}

func TestReplayOnAMissingLogIsNotAnError(t *testing.T) {
	l := &eventLog{path: filepath.Join(t.TempDir(), "never-written.jsonl")}
	if got := collect(t, l, 0, 0); len(got) != 0 {
		t.Fatalf("a missing log yielded %d events", len(got))
	}
}

func TestALargeToolResultSurvivesTheRoundTrip(t *testing.T) {
	// Tool output is what makes lines big. A line the agent stream accepted must be
	// a line the log can read back, or the transcript loses exactly the events that
	// carry the evidence.
	l := openTestLog(t)
	big := strings.Repeat("x", 1<<20)
	if _, err := l.append(Event{Kind: KindToolResult, Tool: "graphit_ast_query", Detail: big}); err != nil {
		t.Fatalf("append: %v", err)
	}
	got := collect(t, l, 0, 0)
	if len(got) != 1 {
		t.Fatalf("replay yielded %d events, want 1", len(got))
	}
	if len(got[0].Detail) != len(big) {
		t.Fatalf("the payload came back %d bytes, want %d", len(got[0].Detail), len(big))
	}
}

func TestReopeningALogContinuesTheSequence(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, eventsFileName)

	first, err := openEventLog(path)
	if err != nil {
		t.Fatalf("openEventLog: %v", err)
	}
	for i := 0; i < 2; i++ {
		if _, err := first.append(Event{Kind: KindText}); err != nil {
			t.Fatalf("append: %v", err)
		}
	}
	if err := first.close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	second, err := openEventLog(path)
	if err != nil {
		t.Fatalf("reopening: %v", err)
	}
	defer func() { _ = second.close() }()

	ev, err := second.append(Event{Kind: KindText})
	if err != nil {
		t.Fatalf("append: %v", err)
	}
	if ev.Seq != 3 {
		t.Fatalf("after reopening, the next event got seq %d, want 3", ev.Seq)
	}
}

func TestClosingTwiceIsHarmless(t *testing.T) {
	l := openTestLog(t)
	if err := l.close(); err != nil {
		t.Fatalf("first close: %v", err)
	}
	if err := l.close(); err != nil {
		t.Fatalf("second close: %v", err)
	}
}

func TestEventFromAIMapsWhatSubscribersNeedAndDropsPlumbing(t *testing.T) {
	cases := []struct {
		in   ai.EventKind
		want Kind
		ok   bool
	}{
		{ai.EventText, KindText, true},
		{ai.EventThinking, KindThinking, true},
		{ai.EventToolUse, KindToolUse, true},
		{ai.EventToolResult, KindToolResult, true},
		{ai.EventStderr, KindStderr, true},
		{ai.EventError, KindError, true},
		// The agent's own conversation ID is a handle to a live session; publishing
		// it serves nobody. Its "done" ends one CLI invocation, not the turn.
		{ai.EventSession, "", false},
		{ai.EventDone, "", false},
		{ai.EventKind("something-a-future-release-invented"), "", false},
	}
	for _, c := range cases {
		got, ok := eventFromAI(ai.Event{Kind: c.in, Text: "t", Tool: "u", Detail: "d"})
		if ok != c.ok {
			t.Fatalf("%s: published=%v, want %v", c.in, ok, c.ok)
		}
		if !ok {
			continue
		}
		if got.Kind != c.want {
			t.Fatalf("%s mapped to %q, want %q", c.in, got.Kind, c.want)
		}
		if got.Text != "t" || got.Tool != "u" || got.Detail != "d" {
			t.Fatalf("%s lost its payload: %+v", c.in, got)
		}
	}
}

func TestSessionIDIsNeverCopiedIntoAPublishedEvent(t *testing.T) {
	// Even for a kind that is published: if a CLI ever attaches a session ID to a
	// text event, it must not ride along into the log.
	got, ok := eventFromAI(ai.Event{Kind: ai.EventText, Text: "hello", SessionID: "secret-handle"})
	if !ok {
		t.Fatal("a text event must be published")
	}
	raw := got.Text + got.Tool + got.Detail + string(got.Kind)
	if strings.Contains(raw, "secret-handle") {
		t.Fatal("the agent's conversation ID reached a published event")
	}
}
