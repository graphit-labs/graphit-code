package livesearch

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// The event log is what makes a live search survivable.
//
// A run lasts minutes and produces hundreds of events, while the HTTP connection
// carrying them lasts as long as a laptop lid stays open. Keeping events only in
// memory and only in the connection means a reconnect shows an empty screen for a
// session that is still working — the run continued, the story of it did not.
//
// So every event is appended to a file first and broadcast second, and every event
// carries a sequence number that is also its SSE id. A client that reconnects sends
// Last-Event-ID and gets the tail it missed, which is behaviour the browser's
// EventSource gives us for free once the ids are there and monotonic.

// maxEventLine bounds a single log line. Tool results are the large ones; this
// matches the ceiling the agent stream reader uses so a line it accepted cannot be
// a line we fail to read back.
const maxEventLine = 8 << 20

// Kind classifies a session event.
//
// This is deliberately its own vocabulary rather than a re-export of ai.EventKind:
// the log carries lifecycle and preparation events the agent knows nothing about,
// and it must not carry one thing the agent does emit — see eventFromAI.
type Kind string

const (
	// KindState reports that the session moved to a new state.
	KindState Kind = "state"
	// KindPrep reports progress while the ephemeral project is being built:
	// artifacts installed, indexes compiled. Preparation is the slowest part of a
	// live search, so it is narrated rather than hidden behind a spinner.
	KindPrep Kind = "prep"
	// KindPrompt echoes the user's message into the log, so a client that
	// reconnects can rebuild the whole transcript from the log alone instead of
	// remembering what it sent.
	KindPrompt Kind = "prompt"
	// KindText is a chunk of the agent's answer.
	KindText Kind = "text"
	// KindThinking is reasoning the agent surfaced separately from its answer.
	KindThinking Kind = "thinking"
	// KindToolUse reports a tool the agent invoked.
	KindToolUse Kind = "tool_use"
	// KindToolResult reports what a tool returned.
	KindToolResult Kind = "tool_result"
	// KindStderr is diagnostic output from the agent process.
	KindStderr Kind = "stderr"
	// KindError reports a failure. It does not necessarily end the session.
	KindError Kind = "error"
	// KindTurnDone closes one turn. The session stays alive for the next one.
	KindTurnDone Kind = "turn_done"
)

// Event is one thing that happened in a session.
type Event struct {
	// Seq is monotonic within a session, starts at 1, and is the SSE event id.
	Seq  int64 `json:"seq"`
	Kind Kind  `json:"kind"`
	// Text carries the payload for text, thinking, prompt, prep, stderr and error.
	Text string `json:"text,omitempty"`
	// Tool is the tool name for tool_use and tool_result.
	Tool string `json:"tool,omitempty"`
	// Detail carries a tool's input or output, already rendered for display.
	Detail string `json:"detail,omitempty"`
	// State is set on state events.
	State State     `json:"state,omitempty"`
	At    time.Time `json:"at"`
}

// eventFromAI translates an agent event into a session event, reporting false for
// the ones that must not reach a subscriber.
//
// Two are dropped on purpose. ai.EventSession carries the CLI's own conversation
// ID: the session records it to resume the next turn, and publishing it would put
// a handle to a live agent conversation on the wire for no one's benefit.
// ai.EventDone ends one CLI invocation, which is not the same event as the turn
// being over — the session emits KindTurnDone itself, after it has recorded the
// outcome, so that "done" always arrives after the error that explains it.
func eventFromAI(ev ai.Event) (Event, bool) {
	var k Kind
	switch ev.Kind {
	case ai.EventText:
		k = KindText
	case ai.EventThinking:
		k = KindThinking
	case ai.EventToolUse:
		k = KindToolUse
	case ai.EventToolResult:
		k = KindToolResult
	case ai.EventStderr:
		k = KindStderr
	case ai.EventError:
		k = KindError
	default:
		return Event{}, false
	}
	return Event{
		Kind:   k,
		Text:   ev.Text,
		Tool:   ev.Tool,
		Detail: ev.Detail,
		At:     ev.At,
	}, true
}

// eventLog is an append-only JSONL file, one event per line.
type eventLog struct {
	path string

	mu   sync.Mutex
	f    *os.File
	last int64
}

// openEventLog opens or recovers a log.
func openEventLog(path string) (*eventLog, error) {
	last, err := recoverLastSeq(path)
	if err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return nil, fmt.Errorf("opening event log: %w", err)
	}
	l := &eventLog{path: path, f: f, last: last}
	if err := l.repairTrailingNewline(); err != nil {
		_ = f.Close()
		return nil, err
	}
	return l, nil
}

// recoverLastSeq reads the highest sequence number already recorded.
//
// It trusts the number inside each record rather than counting lines. The two agree
// on a clean file and disagree on a file whose last write was cut short by a kill,
// and in that case counting lines assigns an already-used sequence number to the
// next event — two events sharing one SSE id, which a reconnecting client resolves
// by silently skipping one of them.
func recoverLastSeq(path string) (int64, error) {
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil
		}
		return 0, fmt.Errorf("reading event log: %w", err)
	}
	defer func() { _ = f.Close() }()

	var last int64
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64*1024), maxEventLine)
	for sc.Scan() {
		var ev Event
		if err := json.Unmarshal(sc.Bytes(), &ev); err != nil {
			continue
		}
		if ev.Seq > last {
			last = ev.Seq
		}
	}
	if err := sc.Err(); err != nil {
		// A log we cannot finish reading is still a log we can append to: the
		// recovered high-water mark is a lower bound, and refusing to open the
		// session would lose a run that is otherwise fine.
		return last, nil
	}
	return last, nil
}

// repairTrailingNewline makes sure the next append starts on its own line.
//
// Without it a truncated final write and the next event become one unparseable
// line, which costs two events instead of the one already lost.
func (l *eventLog) repairTrailingNewline() error {
	st, err := l.f.Stat()
	if err != nil || st.Size() == 0 {
		return nil //nolint:nilerr // an unstattable log is not worth refusing to run
	}
	r, err := os.Open(l.path)
	if err != nil {
		return nil //nolint:nilerr
	}
	defer func() { _ = r.Close() }()

	buf := make([]byte, 1)
	if _, err := r.ReadAt(buf, st.Size()-1); err != nil {
		return nil //nolint:nilerr
	}
	if buf[0] == '\n' {
		return nil
	}
	if _, err := l.f.Write([]byte("\n")); err != nil {
		return fmt.Errorf("repairing event log: %w", err)
	}
	return nil
}

// append assigns the next sequence number and writes the event.
func (l *eventLog) append(ev Event) (Event, error) {
	l.mu.Lock()
	defer l.mu.Unlock()

	if ev.At.IsZero() {
		ev.At = time.Now().UTC()
	}
	ev.Seq = l.last + 1

	line, err := json.Marshal(ev)
	if err != nil {
		// Nothing was written, so the number was never spent.
		return Event{}, fmt.Errorf("encoding event: %w", err)
	}
	l.last = ev.Seq

	if _, err := l.f.Write(append(line, '\n')); err != nil {
		// The number stays spent even though the write failed: part of the line
		// may have landed, and reusing the number would put two different events
		// on the same SSE id. A gap in the sequence costs nothing, because
		// subscribers filter by "greater than", never by "exactly the next one".
		return ev, fmt.Errorf("writing event: %w", err)
	}
	return ev, nil
}

func (l *eventLog) lastSeq() int64 {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.last
}

// errStopReplay ends a replay early without reporting a failure.
var errStopReplay = errors.New("replay stopped")

// replay yields recorded events with after < Seq <= upto, in order. An upto of
// zero or less means no upper bound.
//
// Undecodable lines are skipped rather than fatal: one corrupt record from an
// interrupted write should cost that record and not the rest of the history.
func (l *eventLog) replay(after, upto int64, fn func(Event) error) error {
	f, err := os.Open(l.path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("reading event log: %w", err)
	}
	defer func() { _ = f.Close() }()

	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64*1024), maxEventLine)
	for sc.Scan() {
		var ev Event
		if err := json.Unmarshal(sc.Bytes(), &ev); err != nil {
			continue
		}
		if ev.Seq <= after {
			continue
		}
		if upto > 0 && ev.Seq > upto {
			break
		}
		if err := fn(ev); err != nil {
			if errors.Is(err, errStopReplay) {
				return nil
			}
			return err
		}
	}
	return nil
}

func (l *eventLog) close() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.f == nil {
		return nil
	}
	err := l.f.Close()
	l.f = nil
	if err != nil {
		return fmt.Errorf("closing event log: %w", err)
	}
	return nil
}
