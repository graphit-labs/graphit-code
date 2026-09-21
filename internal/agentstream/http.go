// Package agentstream adds an opt-in progress stream to existing JSON handlers.
package agentstream

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type response struct {
	header http.Header
	status int
	body   bytes.Buffer
}

func (r *response) Header() http.Header { return r.header }
func (r *response) WriteHeader(status int) {
	if r.status == 0 {
		r.status = status
	}
}
func (r *response) Write(b []byte) (int, error) {
	if r.status == 0 {
		r.status = 200
	}
	return r.body.Write(b)
}

// Serve leaves ordinary JSON clients unchanged. Streaming clients receive public
// progress followed by one final JSON response (or error) and done. Disconnects
// cancel the same request context used by the CLI process.
func Serve(w http.ResponseWriter, r *http.Request, handler http.HandlerFunc) {
	if !strings.Contains(r.Header.Get("Accept"), "text/event-stream") {
		handler(w, r)
		return
	}
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "streaming unavailable", 500)
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("X-Accel-Buffering", "no")
	var mu sync.Mutex
	send := func(name string, value any) {
		mu.Lock()
		defer mu.Unlock()
		if r.Context().Err() != nil {
			return
		}
		data, err := json.Marshal(value)
		if err != nil {
			return
		}
		fmt.Fprintf(w, "event: %s\ndata: %s\n\n", name, data)
		flusher.Flush()
	}
	send("progress", ai.Event{Kind: ai.EventThinking, Text: "Starting agent…", At: time.Now().UTC()})
	ctx := ai.WithEventObserver(r.Context(), func(e ai.Event) {
		switch e.Kind {
		case ai.EventText, ai.EventThinking, ai.EventToolUse, ai.EventToolResult, ai.EventStderr, ai.EventStdout, ai.EventError:
			e.SessionID = ""
			send("progress", e)
		}
	})
	captured := &response{header: make(http.Header)}
	handler(captured, r.WithContext(ctx))
	if r.Context().Err() != nil {
		return
	}
	var result map[string]any
	if err := json.Unmarshal(captured.body.Bytes(), &result); err != nil {
		send("error", map[string]string{"error": "Agent returned an invalid response"})
	} else {
		delete(result, "agent_session_id") // native CLI IDs are server-side state
		if message, _ := result["error"].(string); message != "" {
			send("error", map[string]string{"error": message})
		} else if captured.status >= 400 {
			send("error", map[string]string{"error": http.StatusText(captured.status)})
		} else {
			send("final", result)
		}
	}
	send("done", map[string]bool{"done": true})
}
