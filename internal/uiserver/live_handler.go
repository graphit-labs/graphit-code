package uiserver

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/livesearch/prep"
)

// The live search is exposed over Server-Sent Events rather than as a request that
// returns an answer.
//
// The work outlives any single request — preparing the ephemeral project takes most
// of the time, and the agent then runs for minutes — so the transport's only job is
// to carry a session's events to whoever is currently watching. SSE fits that shape:
// one long GET, ordered events with ids, and a client that reconnects with
// Last-Event-ID and is handed exactly what it missed. Sending is the small half, so
// it stays an ordinary POST.
//
// WebSocket would also work and buys nothing here: there is no high-rate client
// traffic to justify a second protocol, and reconnect-with-resume would have to be
// designed and implemented instead of inherited.

const (
	// sseHeartbeat bounds how long a quiet stream stays silent. Preparation can run
	// for minutes without an event, and an idle connection is exactly what proxies
	// and NATs reap. A comment line costs nothing and also reveals a client that
	// vanished without closing, because writing to it finally fails.
	sseHeartbeat = 25 * time.Second

	// sseRetry tells the browser how long to wait before reconnecting.
	sseRetry = 2 * time.Second
)

// heartbeatInterval is a variable only so a test can shorten it and observe a
// heartbeat instead of trusting that one would eventually arrive.
var heartbeatInterval = sseHeartbeat

// LiveHandler serves the live search API.
type LiveHandler struct {
	mgr *livesearch.Manager
}

// NewLiveHandler wires the API to a session manager.
func NewLiveHandler(mgr *livesearch.Manager) *LiveHandler {
	return &LiveHandler{mgr: mgr}
}

// Manager exposes the session manager so the server can shut sessions down.
func (h *LiveHandler) Manager() *livesearch.Manager { return h.mgr }

func (h *LiveHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("POST /api/live/sessions", corsJSON(h.handleCreate))
	mux.HandleFunc("GET /api/live/sessions", corsJSON(h.handleList))
	mux.HandleFunc("GET /api/live/sessions/{id}", corsJSON(h.handleGet))
	mux.HandleFunc("DELETE /api/live/sessions/{id}", corsJSON(h.handleRemove))
	mux.HandleFunc("POST /api/live/sessions/{id}/messages", corsJSON(h.handleSend))
	mux.HandleFunc("POST /api/live/sessions/{id}/cancel", corsJSON(h.handleCancel))
	mux.HandleFunc("GET /api/live/sessions/{id}/stream", corsSSE(h.handleStream))
}

// corsSSE is the streaming counterpart of corsJSON.
//
// It exists because corsJSON declares application/json, and a stream that says it is
// JSON is a stream EventSource refuses. The rest of the headers are what keeps the
// stream a stream.
func corsSSE(h http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		if isAllowedOrigin(origin) {
			w.Header().Set("Access-Control-Allow-Origin", origin)
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Connection", "keep-alive")
		// Without this an intermediary may buffer the whole response: the run
		// still works, the events all arrive at the end, and it looks exactly
		// like a server that hung.
		w.Header().Set("X-Accel-Buffering", "no")
		w.Header().Set("X-Content-Type-Options", "nosniff")
		h(w, r)
	}
}

type createLiveSessionRequest struct {
	IDE       string                `json:"ide"`
	Title     string                `json:"title,omitempty"`
	Artifacts []livesearch.Artifact `json:"artifacts,omitempty"`
	// Prompt is the question to ask as soon as the workspace is ready.
	Prompt string `json:"prompt,omitempty"`
}

type liveSessionResponse struct {
	livesearch.Meta
	// LastSeq is where the event log currently ends, so a client can subscribe
	// from a known point instead of replaying a session it already read.
	LastSeq int64 `json:"last_seq"`
}

func (h *LiveHandler) handleCreate(w http.ResponseWriter, r *http.Request) {
	var req createLiveSessionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid JSON body", http.StatusBadRequest)
		return
	}
	if strings.TrimSpace(req.IDE) == "" {
		// The IDE decides how the ephemeral project is set up — which rules,
		// skills and MCP configuration the agent will find — so there is no
		// sensible default to guess here.
		http.Error(w, "ide is required", http.StatusBadRequest)
		return
	}
	// Checked before the session exists, so an unsupported IDE is a rejected
	// request rather than a session that fails halfway through preparing.
	if err := prep.ValidateIDE(req.IDE); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	s, err := h.mgr.Create(livesearch.Options{
		IDE:       req.IDE,
		Title:     req.Title,
		Artifacts: req.Artifacts,
		Prompt:    req.Prompt,
	})
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.WriteHeader(http.StatusCreated)
	writeJSON(w, liveSessionResponse{Meta: s.Meta(), LastSeq: s.LastSeq()})
}

func (h *LiveHandler) handleList(w http.ResponseWriter, r *http.Request) {
	sessions, err := h.mgr.List()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if sessions == nil {
		sessions = []livesearch.Meta{}
	}
	writeJSON(w, sessions)
}

func (h *LiveHandler) handleGet(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	meta, err := h.mgr.Meta(id)
	if err != nil {
		writeLiveError(w, err)
		return
	}
	resp := liveSessionResponse{Meta: meta}
	if s, err := h.mgr.Get(id); err == nil {
		resp.LastSeq = s.LastSeq()
	}
	writeJSON(w, resp)
}

func (h *LiveHandler) handleRemove(w http.ResponseWriter, r *http.Request) {
	if err := h.mgr.Remove(r.PathValue("id")); err != nil {
		writeLiveError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

type sendLiveMessageRequest struct {
	Prompt string `json:"prompt"`
}

func (h *LiveHandler) handleSend(w http.ResponseWriter, r *http.Request) {
	s, err := h.mgr.Get(r.PathValue("id"))
	if err != nil {
		writeLiveError(w, err)
		return
	}
	var req sendLiveMessageRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid JSON body", http.StatusBadRequest)
		return
	}
	if err := s.Send(req.Prompt); err != nil {
		writeLiveError(w, err)
		return
	}
	// Accepted, not OK: the answer is not in this response and will never be. It
	// arrives on the stream, which is the only place a turn's output exists.
	w.WriteHeader(http.StatusAccepted)
	writeJSON(w, liveSessionResponse{Meta: s.Meta(), LastSeq: s.LastSeq()})
}

func (h *LiveHandler) handleCancel(w http.ResponseWriter, r *http.Request) {
	s, err := h.mgr.Get(r.PathValue("id"))
	if err != nil {
		writeLiveError(w, err)
		return
	}
	// Cancelling an idle session is not an error: a client cannot know whether the
	// turn ended between rendering the button and the click reaching us.
	s.Cancel()
	w.WriteHeader(http.StatusNoContent)
}

func (h *LiveHandler) handleStream(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "this server cannot stream", http.StatusInternalServerError)
		return
	}

	s, err := h.mgr.Get(id)
	if err != nil {
		// Not live here, but its log may still be on disk — a session created by the
		// CLI, or by a previous run of this server. Its transcript is readable even
		// though nothing will be added to it, and refusing it would make the durable
		// log useless to every client but the one process that wrote it.
		h.replayFromDisk(w, r, flusher, id)
		return
	}

	events, stop := s.Subscribe(lastEventID(r))
	// Unsubscribing is all that happens when the client goes away. The session
	// keeps its goroutine, its context and its work: that separation is the reason
	// this subsystem exists.
	defer stop()

	if _, err := fmt.Fprintf(w, "retry: %d\n\n", sseRetry.Milliseconds()); err != nil {
		return
	}
	flusher.Flush()

	ticker := time.NewTicker(heartbeatInterval)
	defer ticker.Stop()

	for {
		select {
		case ev, open := <-events:
			if !open {
				return
			}
			if err := writeSSEEvent(w, ev); err != nil {
				return
			}
			flusher.Flush()
		case <-ticker.C:
			if _, err := io.WriteString(w, ": keepalive\n\n"); err != nil {
				return
			}
			flusher.Flush()
		case <-r.Context().Done():
			return
		}
	}
}

// replayFromDisk serves the recorded transcript of a session this process does not own,
// then ends the stream.
//
// It closes rather than holding the connection open, because no further event can
// arrive: the goroutine that would produce one belongs to a process that is gone. A
// client left waiting on it would wait forever for something nobody is writing.
func (h *LiveHandler) replayFromDisk(w http.ResponseWriter, r *http.Request, flusher http.Flusher, id string) {
	after := lastEventID(r)
	var wrote bool
	err := h.mgr.Replay(id, after, func(ev livesearch.Event) error {
		if r.Context().Err() != nil {
			return r.Context().Err()
		}
		if err := writeSSEEvent(w, ev); err != nil {
			return err
		}
		wrote = true
		flusher.Flush()
		return nil
	})
	if err != nil && !wrote {
		// Nothing was sent, so the status line is still ours to write.
		writeLiveError(w, err)
	}
}

// writeSSEEvent writes one event in the wire format EventSource expects.
func writeSSEEvent(w io.Writer, ev livesearch.Event) error {
	payload, err := json.Marshal(ev)
	if err != nil {
		return fmt.Errorf("encoding event %d: %w", ev.Seq, err)
	}
	// The payload is JSON on one line on purpose: SSE ends an event at a blank
	// line, so a raw newline inside a value would split one event into two
	// unparseable halves. JSON escaping removes the possibility instead of
	// filtering for it.
	if _, err := fmt.Fprintf(w, "id: %d\nevent: %s\ndata: %s\n\n", ev.Seq, ev.Kind, payload); err != nil {
		return fmt.Errorf("writing event %d: %w", ev.Seq, err)
	}
	return nil
}

// lastEventID reads where the client wants to resume from.
//
// Two sources, because a browser can only supply one of them. EventSource sends the
// Last-Event-ID header by itself, but only on a reconnect it initiated; a client
// that reloaded the page — or a CLI resuming a session it printed earlier — cannot
// set headers on EventSource at all and has to say so in the query string.
func lastEventID(r *http.Request) int64 {
	raw := r.Header.Get("Last-Event-ID")
	if raw == "" {
		raw = r.URL.Query().Get("last_event_id")
	}
	n, err := strconv.ParseInt(strings.TrimSpace(raw), 10, 64)
	if err != nil || n < 0 {
		// An unreadable resume point means "start from the beginning", which
		// replays history the client may already have. Guessing a number instead
		// would skip events, and a gap is the one outcome worth avoiding.
		return 0
	}
	return n
}

// writeLiveError maps a session error to the status that describes it.
//
// The distinctions matter to a client: 409 says "ask again in a moment", 410 says
// "this session is gone, make a new one", and 404 says "you have the wrong id".
// Collapsing them into 500 would leave a UI with nothing to do but show a stack
// trace to a user who asked a question.
func writeLiveError(w http.ResponseWriter, err error) {
	switch {
	case errors.Is(err, livesearch.ErrNotFound):
		http.Error(w, "session not found", http.StatusNotFound)
	case errors.Is(err, livesearch.ErrClosed):
		http.Error(w, err.Error(), http.StatusGone)
	case errors.Is(err, livesearch.ErrPreparing),
		errors.Is(err, livesearch.ErrBusy),
		errors.Is(err, livesearch.ErrFailed):
		http.Error(w, err.Error(), http.StatusConflict)
	default:
		http.Error(w, err.Error(), http.StatusBadRequest)
	}
}
