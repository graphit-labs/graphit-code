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

const (
	sseHeartbeat = 25 * time.Second

	sseRetry = 2 * time.Second
)

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

func corsSSE(h http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		if isAllowedOrigin(origin) {
			w.Header().Set("Access-Control-Allow-Origin", origin)
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Connection", "keep-alive")
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
		http.Error(w, "ide is required", http.StatusBadRequest)
		return
	}
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
		h.replayFromDisk(w, r, flusher, id)
		return
	}

	events, stop := s.Subscribe(lastEventID(r))
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
		writeLiveError(w, err)
	}
}

func writeSSEEvent(w io.Writer, ev livesearch.Event) error {
	payload, err := json.Marshal(ev)
	if err != nil {
		return fmt.Errorf("encoding event %d: %w", ev.Seq, err)
	}
	if _, err := fmt.Fprintf(w, "id: %d\nevent: %s\ndata: %s\n\n", ev.Seq, ev.Kind, payload); err != nil {
		return fmt.Errorf("writing event %d: %w", ev.Seq, err)
	}
	return nil
}

func lastEventID(r *http.Request) int64 {
	raw := r.Header.Get("Last-Event-ID")
	if raw == "" {
		raw = r.URL.Query().Get("last_event_id")
	}
	n, err := strconv.ParseInt(strings.TrimSpace(raw), 10, 64)
	if err != nil || n < 0 {
		return 0
	}
	return n
}

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
