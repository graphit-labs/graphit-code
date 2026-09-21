package agentstream

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestProgressBeforeFinalAndJSONCompatibility(t *testing.T) {
	release := make(chan struct{})
	handler := func(w http.ResponseWriter, r *http.Request) {
		<-release
		fmt.Fprint(w, `{"answer":"ready","agent_session_id":"private-native-id"}`)
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { Serve(w, r, handler) }))
	defer server.Close()
	req, _ := http.NewRequest("POST", server.URL, nil)
	req.Header.Set("Accept", "text/event-stream")
	response, err := server.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	reader := bufio.NewReader(response.Body)
	first, err := reader.ReadString('\n')
	if err != nil || first != "event: progress\n" {
		t.Fatalf("not incremental: %q %v", first, err)
	}
	close(release)
	var body strings.Builder
	for {
		line, e := reader.ReadString('\n')
		body.WriteString(line)
		if e != nil {
			break
		}
	}
	response.Body.Close()
	if !strings.Contains(body.String(), `"answer":"ready"`) || strings.Contains(body.String(), "private-native-id") {
		t.Fatal(body.String())
	}
	recorder := httptest.NewRecorder()
	Serve(recorder, httptest.NewRequest("POST", "/", nil), handler)
	if !strings.Contains(recorder.Body.String(), "private-native-id") {
		t.Fatal("JSON compatibility lost")
	}
}

func TestStreamError(t *testing.T) {
	recorder := httptest.NewRecorder()
	req := httptest.NewRequest("POST", "/", nil)
	req.Header.Set("Accept", "text/event-stream")
	Serve(recorder, req, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(422)
		fmt.Fprint(w, `{"error":"invalid question"}`)
	})
	body := recorder.Body.String()
	if !strings.Contains(body, "event: error") || strings.Contains(body, "event: final") || !strings.Contains(body, "invalid question") {
		t.Fatal(body)
	}
}

// A Recorder cannot reproduce net/http closing unread bodies after Flush.
func TestStreamReadsPOSTBeforeFlushingHTTP1(t *testing.T) {
	release := make(chan struct{})
	var once sync.Once
	unblock := func() { once.Do(func() { close(release) }) }
	defer unblock()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		Serve(w, r, func(w http.ResponseWriter, r *http.Request) {
			var payload map[string]string
			if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
				fmt.Fprintf(w, `{"error":%q}`, err.Error())
				return
			}
			select {
			case <-release:
			case <-r.Context().Done():
				return
			}
			json.NewEncoder(w).Encode(payload)
		})
	}))
	defer server.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, "POST", server.URL, strings.NewReader(`{"dir":"wiki","query":"find architecture","project_dir":"/workspace/selected"}`))
	req.Header.Set("Accept", "text/event-stream")
	res, err := server.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	reader := bufio.NewReader(res.Body)
	first, err := reader.ReadString('\n')
	if err != nil || first != "event: progress\n" {
		t.Fatalf("first event: %q %v", first, err)
	}
	// Release without closing: defer also unblocks any early failure.
	unblock()
	body, err := io.ReadAll(reader)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{`"dir":"wiki"`, `"query":"find architecture"`, `"project_dir":"/workspace/selected"`, "event: final", "event: done"} {
		if !strings.Contains(string(body), want) {
			t.Fatalf("missing %s in %s", want, body)
		}
	}
	if strings.Contains(string(body), "event: error") {
		t.Fatal(string(body))
	}
}

func TestStreamRejectsUnreadableOrOversizedBodyBeforeHandler(t *testing.T) {
	for _, tc := range []struct {
		name   string
		body   io.ReadCloser
		status int
	}{
		{"oversized", io.NopCloser(strings.NewReader(strings.Repeat("x", maxRequestBytes+1))), http.StatusRequestEntityTooLarge},
		{"broken", failingBody{}, http.StatusBadRequest},
	} {
		t.Run(tc.name, func(t *testing.T) {
			req := httptest.NewRequest("POST", "/", nil)
			req.Body = tc.body
			req.Header.Set("Accept", "text/event-stream")
			rec := httptest.NewRecorder()
			called := false
			Serve(rec, req, func(http.ResponseWriter, *http.Request) { called = true })
			if called || rec.Code != tc.status || rec.Header().Get("Content-Type") != "application/json" {
				t.Fatalf("called=%v code=%d body=%s", called, rec.Code, rec.Body)
			}
		})
	}
}

type failingBody struct{}

func (failingBody) Read([]byte) (int, error) { return 0, io.ErrUnexpectedEOF }
func (failingBody) Close() error             { return nil }
