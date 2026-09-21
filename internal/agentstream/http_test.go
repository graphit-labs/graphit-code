package agentstream

import (
	"bufio"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
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
