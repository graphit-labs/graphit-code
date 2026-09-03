package livesearch

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type fakeClient struct {
	stream func(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error)
}

func (f *fakeClient) Complete(context.Context, string, string) (string, error) {
	return "", errors.New("not used")
}

func (f *fakeClient) SupportsStructuredStream() bool { return true }

func (f *fakeClient) CompleteStream(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	if f.stream == nil {
		return &ai.StreamResult{}, nil
	}
	return f.stream(ctx, req, emit)
}

func echoClient() *fakeClient {
	return &fakeClient{stream: func(_ context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		emit(ai.Event{Kind: ai.EventText, Text: "answer to " + req.UserPrompt})
		emit(ai.Event{Kind: ai.EventSession, SessionID: "cli-123"})
		emit(ai.Event{Kind: ai.EventDone})
		return &ai.StreamResult{Text: "answer to " + req.UserPrompt, SessionID: "cli-123", Structured: true}, nil
	}}
}

func newTestManager(t *testing.T, client ai.StreamClient, prepare PrepareFunc) *Manager {
	t.Helper()
	m := NewManager(t.TempDir(), client, prepare)
	t.Cleanup(m.CloseAll)
	return m
}

type recorder struct {
	mu   sync.Mutex
	reqs []ai.StreamRequest
}

func (r *recorder) add(req ai.StreamRequest) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.reqs = append(r.reqs, req)
}

func (r *recorder) count() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	return len(r.reqs)
}

func (r *recorder) at(i int) ai.StreamRequest {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.reqs[i]
}

func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if cond() {
			return
		}
		time.Sleep(2 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s", what)
}

func drain(ch <-chan Event) []Event {
	var out []Event
	for ev := range ch {
		out = append(out, ev)
	}
	return out
}

func TestCreateReachesReadyAndPersistsMetadata(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)

	s, err := m.Create(Options{IDE: "claude", Title: "why is it slow"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	if _, err := os.Stat(s.WorkspaceDir()); err != nil {
		t.Fatalf("workspace directory missing: %v", err)
	}
	if s.WorkspaceDir() == s.Dir() {
		t.Fatal("workspace must be below the session directory, not equal to it")
	}

	meta, err := loadMeta(filepath.Join(s.Dir(), metaFileName))
	if err != nil {
		t.Fatalf("loadMeta: %v", err)
	}
	if meta.State != StateReady || meta.IDE != "claude" || meta.Title != "why is it slow" {
		t.Fatalf("persisted metadata is wrong: %+v", meta)
	}
}

func TestTurnOutlivesTheSubscriber(t *testing.T) {
	release := make(chan struct{})
	client := &fakeClient{stream: func(_ context.Context, _ ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		<-release
		emit(ai.Event{Kind: ai.EventText, Text: "finished anyway"})
		return &ai.StreamResult{Text: "finished anyway"}, nil
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	ch, stop := s.Subscribe(0)
	if err := s.Send("question"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "running", func() bool { return s.State() == StateRunning })

	stop()
	drain(ch)
	close(release)

	waitFor(t, "back to ready", func() bool { return s.State() == StateReady })

	var texts []string
	if err := s.log.replay(0, 0, func(ev Event) error {
		if ev.Kind == KindText {
			texts = append(texts, ev.Text)
		}
		return nil
	}); err != nil {
		t.Fatalf("replay: %v", err)
	}
	if len(texts) != 1 || texts[0] != "finished anyway" {
		t.Fatalf("the run did not complete after the subscriber left: %v", texts)
	}
}

func TestSubscribeReplaysAfterLastEventID(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })
	if err := s.Send("first"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "turn done", func() bool { return s.State() == StateReady && s.LastSeq() > 3 })

	last := s.LastSeq()

	ch, stop := s.Subscribe(3)
	defer stop()

	var got []Event
	deadline := time.After(5 * time.Second)
	for int64(len(got)) < last-3 {
		select {
		case ev := <-ch:
			got = append(got, ev)
		case <-deadline:
			t.Fatalf("only replayed %d of %d events", len(got), last-3)
		}
	}
	for i, ev := range got {
		if want := int64(4 + i); ev.Seq != want {
			t.Fatalf("event %d has seq %d, want %d", i, ev.Seq, want)
		}
	}
}

func TestSubscribeMidRunHasNoGapAndNoDuplicate(t *testing.T) {
	const emitted = 400
	started := make(chan struct{})
	client := &fakeClient{stream: func(_ context.Context, _ ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		close(started)
		for i := 0; i < emitted; i++ {
			emit(ai.Event{Kind: ai.EventText, Text: fmt.Sprintf("chunk-%d", i)})
		}
		return &ai.StreamResult{}, nil
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	var (
		mu   sync.Mutex
		got  []Event
		done = make(chan struct{})
	)
	go func() {
		<-started
		ch, stop := s.Subscribe(0)
		defer stop()
		defer close(done)
		for ev := range ch {
			mu.Lock()
			got = append(got, ev)
			mu.Unlock()
			if ev.Kind == KindTurnDone {
				return
			}
		}
	}()

	if err := s.Send("go"); err != nil {
		t.Fatalf("Send: %v", err)
	}

	select {
	case <-done:
	case <-time.After(10 * time.Second):
		t.Fatal("subscriber never saw the turn finish")
	}

	mu.Lock()
	defer mu.Unlock()
	if len(got) == 0 {
		t.Fatal("subscriber received nothing")
	}
	seen := make(map[int64]bool, len(got))
	prev := int64(0)
	for _, ev := range got {
		if seen[ev.Seq] {
			t.Fatalf("event %d delivered twice", ev.Seq)
		}
		seen[ev.Seq] = true
		if ev.Seq <= prev {
			t.Fatalf("out of order: %d after %d", ev.Seq, prev)
		}
		prev = ev.Seq
	}
	for want := int64(1); want <= prev; want++ {
		if !seen[want] {
			t.Fatalf("event %d is missing from a stream that reached %d", want, prev)
		}
	}
}

func TestSlowSubscriberIsDroppedWithoutStallingTheRun(t *testing.T) {
	const emitted = tailBuffer * 3
	client := &fakeClient{stream: func(_ context.Context, _ ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		for i := 0; i < emitted; i++ {
			emit(ai.Event{Kind: ai.EventText, Text: fmt.Sprintf("chunk-%d", i)})
		}
		return &ai.StreamResult{}, nil
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	ch, stop := s.Subscribe(0)
	defer stop()

	if err := s.Send("go"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "the run to finish despite the stalled subscriber", func() bool {
		return s.State() == StateReady && s.LastSeq() >= int64(emitted)
	})

	waitFor(t, "the stalled subscriber to be closed", func() bool {
		for {
			select {
			case _, ok := <-ch:
				if !ok {
					return true
				}
			default:
				return false
			}
		}
	})
}

func TestSendIsRejectedWithAReasonPerState(t *testing.T) {
	gate := make(chan struct{})
	prepare := func(context.Context, *Session, func(string)) error {
		<-gate
		return nil
	}
	m := newTestManager(t, echoClient(), prepare)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}

	if err := s.Send("too early"); !errors.Is(err, ErrPreparing) {
		t.Fatalf("while preparing, Send returned %v, want ErrPreparing", err)
	}
	close(gate)
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	if err := s.Send("  "); err == nil {
		t.Fatal("an empty prompt should be refused")
	}

	blocked := make(chan struct{})
	busy := &fakeClient{stream: func(context.Context, ai.StreamRequest, ai.EventFunc) (*ai.StreamResult, error) {
		<-blocked
		return &ai.StreamResult{}, nil
	}}
	s.client = busy
	if err := s.Send("first"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "running", func() bool { return s.State() == StateRunning })
	if err := s.Send("second"); !errors.Is(err, ErrBusy) {
		t.Fatalf("while running, Send returned %v, want ErrBusy", err)
	}
	close(blocked)
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	if err := s.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	if err := s.Send("after close"); !errors.Is(err, ErrClosed) {
		t.Fatalf("after Close, Send returned %v, want ErrClosed", err)
	}
}

func TestPreparationFailureFailsTheSession(t *testing.T) {
	prepare := func(context.Context, *Session, func(string)) error {
		return errors.New("the hub was unreachable")
	}
	m := newTestManager(t, echoClient(), prepare)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "failed", func() bool { return s.State() == StateFailed })

	if err := s.Send("anything"); !errors.Is(err, ErrFailed) {
		t.Fatalf("Send returned %v, want ErrFailed", err)
	}
	if got := s.Meta().Error; !strings.Contains(got, "hub was unreachable") {
		t.Fatalf("the failure reason was not recorded: %q", got)
	}

	var sawError bool
	if err := s.log.replay(0, 0, func(ev Event) error {
		if ev.Kind == KindError && strings.Contains(ev.Text, "hub was unreachable") {
			sawError = true
		}
		return nil
	}); err != nil {
		t.Fatalf("replay: %v", err)
	}
	if !sawError {
		t.Fatal("the failure was not written to the event log")
	}
}

func TestPreparationProgressIsSurfaced(t *testing.T) {
	prepare := func(_ context.Context, _ *Session, progress func(string)) error {
		progress("installing artefacts")
		progress("compiling the wiki")
		return nil
	}
	m := newTestManager(t, echoClient(), prepare)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	var prep []string
	if err := s.log.replay(0, 0, func(ev Event) error {
		if ev.Kind == KindPrep {
			prep = append(prep, ev.Text)
		}
		return nil
	}); err != nil {
		t.Fatalf("replay: %v", err)
	}
	if len(prep) != 2 || prep[0] != "installing artefacts" || prep[1] != "compiling the wiki" {
		t.Fatalf("preparation progress was lost: %v", prep)
	}
}

func TestTurnErrorKeepsTheSessionUsable(t *testing.T) {
	var rec recorder
	client := &fakeClient{stream: func(_ context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		rec.add(req)
		if rec.count() == 1 {
			return nil, errors.New("the CLI exited 1")
		}
		emit(ai.Event{Kind: ai.EventText, Text: "second time worked"})
		return &ai.StreamResult{Text: "second time worked"}, nil
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	if err := s.Send("first"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "ready after the failed turn", func() bool {
		return s.State() == StateReady && rec.count() == 1
	})
	if s.State() == StateFailed {
		t.Fatal("a failed turn must not fail the whole session")
	}
	if err := s.Send("second"); err != nil {
		t.Fatalf("the session was unusable after one failed turn: %v", err)
	}
}

func TestCancelStopsTheTurnAndLeavesTheSessionReady(t *testing.T) {
	client := &fakeClient{stream: func(ctx context.Context, _ ai.StreamRequest, _ ai.EventFunc) (*ai.StreamResult, error) {
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	if err := s.Send("long one"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "running", func() bool { return s.State() == StateRunning })
	s.Cancel()
	waitFor(t, "ready again", func() bool { return s.State() == StateReady })

	var cancelled bool
	if err := s.log.replay(0, 0, func(ev Event) error {
		if ev.Kind == KindError && ev.Text == "cancelled" {
			cancelled = true
		}
		return nil
	}); err != nil {
		t.Fatalf("replay: %v", err)
	}
	if !cancelled {
		t.Fatal("cancelling was not recorded")
	}
}

func TestTheCLISessionIDIsCarriedIntoTheNextTurn(t *testing.T) {
	var rec recorder
	client := &fakeClient{stream: func(_ context.Context, req ai.StreamRequest, _ ai.EventFunc) (*ai.StreamResult, error) {
		rec.add(req)
		return &ai.StreamResult{SessionID: "cli-abc"}, nil
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	for i := 0; i < 2; i++ {
		want := i + 1
		if err := s.Send(fmt.Sprintf("turn %d", i)); err != nil {
			t.Fatalf("Send: %v", err)
		}
		waitFor(t, "ready", func() bool { return s.State() == StateReady && rec.count() == want })
	}
	if got := rec.at(0).SessionID; got != "" {
		t.Fatalf("the first turn should start a new conversation, got %q", got)
	}
	if got := rec.at(1).SessionID; got != "cli-abc" {
		t.Fatalf("the second turn did not resume the conversation: %q", got)
	}
}

func TestTheAgentRunsInsideTheWorkspaceWithToolsAllowed(t *testing.T) {
	var rec recorder
	client := &fakeClient{stream: func(_ context.Context, req ai.StreamRequest, _ ai.EventFunc) (*ai.StreamResult, error) {
		rec.add(req)
		return &ai.StreamResult{}, nil
	}}
	m := newTestManager(t, client, nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })
	if err := s.Send("where is the parser"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "the turn to run", func() bool { return rec.count() == 1 })

	got := rec.at(0)
	if got.WorkDir != s.WorkspaceDir() {
		t.Fatalf("the agent ran in %q, want the workspace %q", got.WorkDir, s.WorkspaceDir())
	}
	if !got.AllowTools {
		t.Fatal("a live search agent that cannot use tools cannot search")
	}
}

func TestTheCLIConversationIDIsNeverPublished(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })
	if err := s.Send("hello"); err != nil {
		t.Fatalf("Send: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady && s.LastSeq() > 4 })

	raw, err := os.ReadFile(filepath.Join(s.Dir(), eventsFileName))
	if err != nil {
		t.Fatalf("reading the log: %v", err)
	}
	if strings.Contains(string(raw), "cli-123") {
		t.Fatal("the agent's own conversation ID leaked into the published event log")
	}
	s.mu.Lock()
	got := s.cliSessionID
	s.mu.Unlock()
	if got != "cli-123" {
		t.Fatalf("the conversation ID was not captured: %q", got)
	}
}

func TestRemoveDeletesEverythingIncludingTheWorkspace(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })

	marker := filepath.Join(s.WorkspaceDir(), "AGENTS.md")
	if err := os.WriteFile(marker, []byte("mandate"), 0o600); err != nil {
		t.Fatalf("seeding the workspace: %v", err)
	}

	if err := m.Remove(s.ID()); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if _, err := os.Stat(s.Dir()); !os.IsNotExist(err) {
		t.Fatalf("the session directory survived removal: %v", err)
	}
	if _, err := m.Get(s.ID()); !errors.Is(err, ErrNotFound) {
		t.Fatalf("Get after Remove returned %v, want ErrNotFound", err)
	}
}

func TestRemoveRefusesIDsThatAreNotSessionIDs(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)
	victim := filepath.Join(filepath.Dir(m.Root()), "precious")
	if err := os.MkdirAll(victim, 0o750); err != nil {
		t.Fatalf("setup: %v", err)
	}

	for _, id := range []string{"..", "../precious", "", strings.Repeat("A", 26) + "B", "01ARZ3NDEKTSV4RRFFQ69G5FA/"} {
		if err := m.Remove(id); !errors.Is(err, ErrNotFound) {
			t.Fatalf("Remove(%q) returned %v, want ErrNotFound", id, err)
		}
	}
	if _, err := os.Stat(victim); err != nil {
		t.Fatalf("a traversal reached outside the sessions root: %v", err)
	}
}

func TestListReportsInterruptedSessionsAsFailed(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)

	id := newSessionID()
	dir := filepath.Join(m.Root(), id)
	if err := os.MkdirAll(dir, 0o750); err != nil {
		t.Fatalf("setup: %v", err)
	}
	meta := Meta{ID: id, State: StateRunning, IDE: "claude", CreatedAt: time.Now().UTC()}
	data, err := json.Marshal(meta)
	if err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, metaFileName), data, 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}

	live, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return live.State() == StateReady })

	list, err := m.List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(list) != 2 {
		t.Fatalf("List returned %d sessions, want 2", len(list))
	}
	byID := map[string]Meta{}
	for _, meta := range list {
		byID[meta.ID] = meta
	}
	if got := byID[id].State; got != StateFailed {
		t.Fatalf("an abandoned session is reported as %q, want failed", got)
	}
	if got := byID[live.ID()].State; got != StateReady {
		t.Fatalf("the live session is reported as %q, want ready", got)
	}
}

func TestCloseAllStopsEverySession(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)
	for i := 0; i < 3; i++ {
		if _, err := m.Create(Options{IDE: "claude"}); err != nil {
			t.Fatalf("Create: %v", err)
		}
	}
	m.CloseAll()
	list, err := m.List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	for _, meta := range list {
		if meta.State != StateClosed {
			t.Fatalf("session %s is %q after CloseAll, want closed", meta.ID, meta.State)
		}
	}
}

func TestSubscribeToAClosedSessionReplaysWithoutHanging(t *testing.T) {
	m := newTestManager(t, echoClient(), nil)
	s, err := m.Create(Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitFor(t, "ready", func() bool { return s.State() == StateReady })
	if err := s.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	ch, stop := s.Subscribe(0)
	defer stop()

	done := make(chan []Event, 1)
	go func() { done <- drain(ch) }()
	select {
	case got := <-done:
		if len(got) == 0 {
			t.Fatal("a closed session replayed nothing")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("subscribing to a closed session hung waiting for events that cannot come")
	}
}
