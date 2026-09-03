package uiserver

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
)

type fakeStreamClient struct {
	stream func(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error)
}

func (f *fakeStreamClient) Complete(context.Context, string, string) (string, error) {
	return "", errors.New("not used")
}

func (f *fakeStreamClient) SupportsStructuredStream() bool { return true }

func (f *fakeStreamClient) CompleteStream(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	if f.stream == nil {
		return &ai.StreamResult{}, nil
	}
	return f.stream(ctx, req, emit)
}

func answering(text string) *fakeStreamClient {
	return &fakeStreamClient{stream: func(_ context.Context, _ ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		emit(ai.Event{Kind: ai.EventText, Text: text})
		return &ai.StreamResult{Text: text}, nil
	}}
}

func newLiveTestServer(t *testing.T, client ai.StreamClient, prepare livesearch.PrepareFunc) (*httptest.Server, *livesearch.Manager) {
	t.Helper()
	mgr := livesearch.NewManager(t.TempDir(), client, prepare)
	t.Cleanup(mgr.CloseAll)

	mux := http.NewServeMux()
	NewLiveHandler(mgr).RegisterAPIRoutes(mux)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return srv, mgr
}

type sseEvent struct {
	id    string
	kind  string
	data  string
	retry string
}

func openStream(t *testing.T, url string, headers map[string]string) (*http.Response, <-chan sseEvent, func()) {
	t.Helper()
	req, err := http.NewRequestWithContext(context.Background(), http.MethodGet, url, nil)
	if err != nil {
		t.Fatalf("building request: %v", err)
	}
	for k, v := range headers {
		req.Header.Set(k, v)
	}
	resp, err := http.DefaultClient.Do(req) //nolint:bodyclose // closed by the returned func
	if err != nil {
		t.Fatalf("opening stream: %v", err)
	}

	out := make(chan sseEvent, 256)
	go func() {
		defer close(out)
		sc := bufio.NewScanner(resp.Body)
		sc.Buffer(make([]byte, 0, 64*1024), 8<<20)
		var cur sseEvent
		for sc.Scan() {
			line := sc.Text()
			switch {
			case line == "":
				if cur.data != "" || cur.retry != "" {
					out <- cur
				}
				cur = sseEvent{}
			case strings.HasPrefix(line, ":"):
				cur.kind = "keepalive"
				cur.data = strings.TrimSpace(strings.TrimPrefix(line, ":"))
			case strings.HasPrefix(line, "id: "):
				cur.id = strings.TrimPrefix(line, "id: ")
			case strings.HasPrefix(line, "event: "):
				cur.kind = strings.TrimPrefix(line, "event: ")
			case strings.HasPrefix(line, "data: "):
				cur.data = strings.TrimPrefix(line, "data: ")
			case strings.HasPrefix(line, "retry: "):
				cur.retry = strings.TrimPrefix(line, "retry: ")
			}
		}
	}()

	return resp, out, func() { _ = resp.Body.Close() }
}

func nextEvent(t *testing.T, events <-chan sseEvent) (sseEvent, bool) {
	t.Helper()
	select {
	case ev, ok := <-events:
		return ev, ok
	case <-time.After(10 * time.Second):
		t.Fatal("timed out waiting for a stream event")
		return sseEvent{}, false
	}
}

func awaitKind(t *testing.T, events <-chan sseEvent, kind string) sseEvent {
	t.Helper()
	for {
		ev, ok := nextEvent(t, events)
		if !ok {
			t.Fatalf("the stream ended before a %q event arrived", kind)
		}
		if ev.kind == kind {
			return ev
		}
	}
}

func createSession(t *testing.T, srv *httptest.Server, body any) liveSessionResponse {
	t.Helper()
	payload, err := json.Marshal(body)
	if err != nil {
		t.Fatalf("encoding request: %v", err)
	}
	resp, err := srv.Client().Post(srv.URL+"/api/live/sessions", "application/json", bytes.NewReader(payload))
	if err != nil {
		t.Fatalf("creating session: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusCreated {
		raw, _ := io.ReadAll(resp.Body)
		t.Fatalf("create returned %d: %s", resp.StatusCode, raw)
	}
	var out liveSessionResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		t.Fatalf("decoding session: %v", err)
	}
	return out
}

func postJSON(t *testing.T, srv *httptest.Server, path string, body any) *http.Response {
	t.Helper()
	var reader io.Reader = strings.NewReader("{}")
	if body != nil {
		payload, err := json.Marshal(body)
		if err != nil {
			t.Fatalf("encoding request: %v", err)
		}
		reader = bytes.NewReader(payload)
	}
	resp, err := srv.Client().Post(srv.URL+path, "application/json", reader)
	if err != nil {
		t.Fatalf("POST %s: %v", path, err)
	}
	return resp
}

func waitForState(t *testing.T, srv *httptest.Server, id string, want livesearch.State) {
	t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	var last livesearch.State
	for time.Now().Before(deadline) {
		resp, err := srv.Client().Get(srv.URL + "/api/live/sessions/" + id)
		if err != nil {
			t.Fatalf("GET session: %v", err)
		}
		var got liveSessionResponse
		err = json.NewDecoder(resp.Body).Decode(&got)
		_ = resp.Body.Close()
		if err == nil {
			last = got.State
			if got.State == want {
				return
			}
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("session stayed in state %q, want %q", last, want)
}

func TestCreateStartsASessionAndReportsIt(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)

	got := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "why is startup slow"})
	if got.ID == "" {
		t.Fatal("create returned no session id")
	}
	if got.IDE != "claude" {
		t.Fatalf("ide is %q, want claude", got.IDE)
	}
	if got.Title != "why is startup slow" {
		t.Fatalf("title is %q, want the prompt", got.Title)
	}
}

func TestCreateRequiresAnIDE(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)

	resp := postJSON(t, srv, "/api/live/sessions", createLiveSessionRequest{Prompt: "no ide"})
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("create without an ide returned %d, want 400", resp.StatusCode)
	}
}

func TestTheInitialPromptRunsWithoutAnyoneAsking(t *testing.T) {
	gate := make(chan struct{})
	prepare := func(context.Context, *livesearch.Session, func(string)) error {
		<-gate
		return nil
	}
	srv, _ := newLiveTestServer(t, answering("because of the indexer"), prepare)

	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "why is startup slow"})
	close(gate)
	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	defer closeStream()

	ev := awaitKind(t, events, string(livesearch.KindText))
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
		t.Fatalf("decoding event: %v", err)
	}
	if decoded.Text != "because of the indexer" {
		t.Fatalf("the answer was %q, want the agent's", decoded.Text)
	}
}

func TestStreamDeclaresItselfAsEventStream(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})

	resp, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	defer closeStream()

	if got := resp.Header.Get("Content-Type"); got != "text/event-stream" {
		t.Fatalf("Content-Type is %q — EventSource refuses anything else", got)
	}
	if got := resp.Header.Get("Cache-Control"); got != "no-cache" {
		t.Fatalf("Cache-Control is %q, want no-cache", got)
	}
	if got := resp.Header.Get("X-Accel-Buffering"); got != "no" {
		t.Fatalf("X-Accel-Buffering is %q, want no", got)
	}

	first, ok := nextEvent(t, events)
	if !ok {
		t.Fatal("the stream closed immediately")
	}
	if first.retry == "" {
		t.Fatalf("the first frame carried no retry hint: %+v", first)
	}
}

func TestEveryEventCarriesItsSequenceAsTheSSEID(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("answer"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	defer closeStream()

	resp := postJSON(t, srv, "/api/live/sessions/"+created.ID+"/messages", sendLiveMessageRequest{Prompt: "ask"})
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("sending a message returned %d, want 202", resp.StatusCode)
	}
	_ = resp.Body.Close()

	seen := 0
	for seen < 3 {
		ev, ok := nextEvent(t, events)
		if !ok {
			t.Fatal("the stream ended early")
		}
		if ev.retry != "" && ev.data == "" {
			continue
		}
		if ev.kind == "keepalive" {
			continue
		}
		if ev.id == "" {
			t.Fatalf("event %+v has no id, so a reconnect cannot resume from it", ev)
		}
		var decoded livesearch.Event
		if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
			t.Fatalf("event data is not JSON: %v", err)
		}
		if fmt.Sprint(decoded.Seq) != ev.id {
			t.Fatalf("the SSE id %q does not match the event sequence %d", ev.id, decoded.Seq)
		}
		seen++
	}
}

func TestTheRunSurvivesTheClientDisconnecting(t *testing.T) {
	release := make(chan struct{})
	var once sync.Once
	client := &fakeStreamClient{stream: func(_ context.Context, _ ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
		<-release
		emit(ai.Event{Kind: ai.EventText, Text: "finished with nobody watching"})
		return &ai.StreamResult{Text: "finished with nobody watching"}, nil
	}}
	srv, _ := newLiveTestServer(t, client, nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	resp := postJSON(t, srv, "/api/live/sessions/"+created.ID+"/messages", sendLiveMessageRequest{Prompt: "long one"})
	_ = resp.Body.Close()
	waitForState(t, srv, created.ID, livesearch.StateRunning)

	closeStream()
	for range events {
	}
	once.Do(func() { close(release) })

	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, again, closeAgain := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	defer closeAgain()

	ev := awaitKind(t, again, string(livesearch.KindText))
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
		t.Fatalf("decoding event: %v", err)
	}
	if decoded.Text != "finished with nobody watching" {
		t.Fatalf("the answer produced while disconnected was lost: %q", decoded.Text)
	}
}

func TestReconnectResumesFromLastEventIDHeader(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("answer"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "ask"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream",
		map[string]string{"Last-Event-ID": "2"})
	defer closeStream()

	ev, ok := nextEvent(t, events)
	for ok && ev.data == "" && ev.retry != "" {
		ev, ok = nextEvent(t, events)
	}
	if !ok {
		t.Fatal("nothing was replayed")
	}
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
		t.Fatalf("decoding event: %v", err)
	}
	if decoded.Seq != 3 {
		t.Fatalf("the replay restarted at %d, want 3 — the client already had 1 and 2", decoded.Seq)
	}
}

func TestReconnectResumesFromTheQueryParameterToo(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("answer"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "ask"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, events, closeStream := openStream(t,
		srv.URL+"/api/live/sessions/"+created.ID+"/stream?last_event_id=2", nil)
	defer closeStream()

	ev, ok := nextEvent(t, events)
	for ok && ev.data == "" && ev.retry != "" {
		ev, ok = nextEvent(t, events)
	}
	if !ok {
		t.Fatal("nothing was replayed")
	}
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
		t.Fatalf("decoding event: %v", err)
	}
	if decoded.Seq != 3 {
		t.Fatalf("the replay restarted at %d, want 3", decoded.Seq)
	}
}

func TestAnUnreadableResumePointReplaysEverything(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("answer"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "ask"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	_, events, closeStream := openStream(t,
		srv.URL+"/api/live/sessions/"+created.ID+"/stream?last_event_id=not-a-number", nil)
	defer closeStream()

	ev, ok := nextEvent(t, events)
	for ok && ev.data == "" && ev.retry != "" {
		ev, ok = nextEvent(t, events)
	}
	if !ok {
		t.Fatal("nothing was replayed")
	}
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
		t.Fatalf("decoding event: %v", err)
	}
	if decoded.Seq != 1 {
		t.Fatalf("replay started at %d — a bad resume point must not skip events", decoded.Seq)
	}
}

func TestAQuietStreamSendsAHeartbeat(t *testing.T) {
	restore := heartbeatInterval
	heartbeatInterval = 20 * time.Millisecond
	defer func() { heartbeatInterval = restore }()

	prepare := func(ctx context.Context, _ *livesearch.Session, _ func(string)) error {
		<-ctx.Done()
		return ctx.Err()
	}
	srv, _ := newLiveTestServer(t, answering("hi"), prepare)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})

	_, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	defer closeStream()

	deadline := time.After(10 * time.Second)
	for {
		select {
		case ev, ok := <-events:
			if !ok {
				t.Fatal("the stream closed before a heartbeat")
			}
			if ev.kind == "keepalive" {
				return
			}
		case <-deadline:
			t.Fatal("no heartbeat on an idle stream")
		}
	}
}

func TestSendingWhileStillPreparingIsAConflict(t *testing.T) {
	gate := make(chan struct{})
	prepare := func(context.Context, *livesearch.Session, func(string)) error {
		<-gate
		return nil
	}
	srv, _ := newLiveTestServer(t, answering("hi"), prepare)
	defer close(gate)

	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})
	resp := postJSON(t, srv, "/api/live/sessions/"+created.ID+"/messages", sendLiveMessageRequest{Prompt: "too early"})
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusConflict {
		t.Fatalf("sending while preparing returned %d, want 409", resp.StatusCode)
	}
}

func TestSendingToAnUnknownSessionIsNotFound(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)

	resp := postJSON(t, srv, "/api/live/sessions/01ARZ3NDEKTSV4RRFFQ69G5FAV/messages",
		sendLiveMessageRequest{Prompt: "hello"})
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("an unknown session returned %d, want 404", resp.StatusCode)
	}
}

func TestAnEmptyPromptIsRejected(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	resp := postJSON(t, srv, "/api/live/sessions/"+created.ID+"/messages", sendLiveMessageRequest{Prompt: "   "})
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("an empty prompt returned %d, want 400", resp.StatusCode)
	}
}

func TestCancelIsAcceptedEvenWhenNothingIsRunning(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	resp := postJSON(t, srv, "/api/live/sessions/"+created.ID+"/cancel", nil)
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("cancel returned %d, want 204", resp.StatusCode)
	}
}

func TestRemoveDeletesTheSession(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	req, err := http.NewRequestWithContext(context.Background(), http.MethodDelete,
		srv.URL+"/api/live/sessions/"+created.ID, nil)
	if err != nil {
		t.Fatalf("building request: %v", err)
	}
	resp, err := srv.Client().Do(req)
	if err != nil {
		t.Fatalf("DELETE: %v", err)
	}
	_ = resp.Body.Close()
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("remove returned %d, want 204", resp.StatusCode)
	}

	after, err := srv.Client().Get(srv.URL + "/api/live/sessions/" + created.ID)
	if err != nil {
		t.Fatalf("GET after remove: %v", err)
	}
	defer func() { _ = after.Body.Close() }()
	if after.StatusCode != http.StatusNotFound {
		t.Fatalf("a removed session still answers with %d, want 404", after.StatusCode)
	}
}

func TestListIsAlwaysAnArray(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)

	resp, err := srv.Client().Get(srv.URL + "/api/live/sessions")
	if err != nil {
		t.Fatalf("GET list: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("reading list: %v", err)
	}
	if strings.TrimSpace(string(raw)) != "[]" {
		t.Fatalf("an empty session list serialised as %q, want []", strings.TrimSpace(string(raw)))
	}
}

func TestListReportsCreatedSessions(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)
	first := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Title: "one"})
	second := createSession(t, srv, createLiveSessionRequest{IDE: "kiro", Title: "two"})

	resp, err := srv.Client().Get(srv.URL + "/api/live/sessions")
	if err != nil {
		t.Fatalf("GET list: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	var list []livesearch.Meta
	if err := json.NewDecoder(resp.Body).Decode(&list); err != nil {
		t.Fatalf("decoding list: %v", err)
	}
	if len(list) != 2 {
		t.Fatalf("the list has %d sessions, want 2", len(list))
	}
	ids := map[string]bool{}
	for _, meta := range list {
		ids[meta.ID] = true
	}
	if !ids[first.ID] || !ids[second.ID] {
		t.Fatalf("the list is missing a session: %+v", list)
	}
}

func TestStreamingAnUnknownSessionIsNotFound(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)

	resp, err := srv.Client().Get(srv.URL + "/api/live/sessions/01ARZ3NDEKTSV4RRFFQ69G5FAV/stream")
	if err != nil {
		t.Fatalf("GET stream: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("streaming an unknown session returned %d, want 404", resp.StatusCode)
	}
}

func TestWriteSSEEventKeepsOneEventOnOneDataLine(t *testing.T) {
	var buf bytes.Buffer
	err := writeSSEEvent(&buf, livesearch.Event{
		Seq:  7,
		Kind: livesearch.KindToolResult,
		Text: "line one\nline two\n\nline three",
	})
	if err != nil {
		t.Fatalf("writeSSEEvent: %v", err)
	}
	frame := buf.String()
	if !strings.HasPrefix(frame, "id: 7\nevent: tool_result\ndata: ") {
		t.Fatalf("unexpected frame start: %q", frame)
	}
	body := strings.TrimSuffix(strings.TrimPrefix(frame, "id: 7\nevent: tool_result\n"), "\n\n")
	if strings.Count(body, "\n") != 0 {
		t.Fatalf("the payload spans more than one line: %q", body)
	}
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(strings.TrimPrefix(body, "data: ")), &decoded); err != nil {
		t.Fatalf("the payload is not decodable JSON: %v", err)
	}
	if decoded.Text != "line one\nline two\n\nline three" {
		t.Fatalf("the newlines did not survive: %q", decoded.Text)
	}
}

func TestLastEventIDPrefersTheHeaderOverTheQuery(t *testing.T) {
	req, err := http.NewRequestWithContext(context.Background(), http.MethodGet,
		"http://localhost/stream?last_event_id=5", nil)
	if err != nil {
		t.Fatalf("building request: %v", err)
	}
	req.Header.Set("Last-Event-ID", "9")
	if got := lastEventID(req); got != 9 {
		t.Fatalf("lastEventID returned %d, want the header's 9", got)
	}
}

func TestWriteLiveErrorMapsEachRefusalToItsOwnStatus(t *testing.T) {
	cases := []struct {
		err  error
		want int
	}{
		{livesearch.ErrNotFound, http.StatusNotFound},
		{livesearch.ErrClosed, http.StatusGone},
		{livesearch.ErrPreparing, http.StatusConflict},
		{livesearch.ErrBusy, http.StatusConflict},
		{livesearch.ErrFailed, http.StatusConflict},
		{errors.New("something else"), http.StatusBadRequest},
	}
	for _, c := range cases {
		rec := httptest.NewRecorder()
		writeLiveError(rec, c.err)
		if rec.Code != c.want {
			t.Fatalf("%v mapped to %d, want %d", c.err, rec.Code, c.want)
		}
	}
}

func TestATranscriptFromAnotherProcessIsStillReadable(t *testing.T) {
	srv, mgr := newLiveTestServer(t, answering("recorded earlier"), nil)

	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "what happened?"})
	waitForState(t, srv, created.ID, livesearch.StateReady)

	mgr.CloseAll()

	_, events, closeStream := openStream(t, srv.URL+"/api/live/sessions/"+created.ID+"/stream", nil)
	defer closeStream()

	var texts []string
	for {
		ev, ok := nextEvent(t, events)
		if !ok {
			break
		}
		if ev.kind != string(livesearch.KindText) {
			continue
		}
		var decoded livesearch.Event
		if err := json.Unmarshal([]byte(ev.data), &decoded); err != nil {
			t.Fatalf("decoding event: %v", err)
		}
		texts = append(texts, decoded.Text)
	}
	if len(texts) == 0 || !strings.Contains(strings.Join(texts, ""), "recorded earlier") {
		t.Fatalf("the recorded answer was not replayed: %v", texts)
	}
}

func TestAReplayFromDiskHonoursTheResumePoint(t *testing.T) {
	srv, mgr := newLiveTestServer(t, answering("answer"), nil)
	created := createSession(t, srv, createLiveSessionRequest{IDE: "claude", Prompt: "ask"})
	waitForState(t, srv, created.ID, livesearch.StateReady)
	mgr.CloseAll()

	_, events, closeStream := openStream(t,
		srv.URL+"/api/live/sessions/"+created.ID+"/stream?last_event_id=2", nil)
	defer closeStream()

	first, ok := nextEvent(t, events)
	if !ok {
		t.Fatal("nothing was replayed")
	}
	var decoded livesearch.Event
	if err := json.Unmarshal([]byte(first.data), &decoded); err != nil {
		t.Fatalf("decoding event: %v", err)
	}
	if decoded.Seq != 3 {
		t.Fatalf("the replay restarted at %d, want 3", decoded.Seq)
	}
}

func TestAnUnknownSessionIsStillNotFoundOnDisk(t *testing.T) {
	srv, _ := newLiveTestServer(t, answering("hi"), nil)

	resp, err := srv.Client().Get(srv.URL + "/api/live/sessions/01ARZ3NDEKTSV4RRFFQ69G5FAV/stream")
	if err != nil {
		t.Fatalf("GET stream: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("an unknown session returned %d, want 404", resp.StatusCode)
	}
}
