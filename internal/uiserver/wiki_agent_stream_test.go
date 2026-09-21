//go:build lancedb

package uiserver

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

type wikiStreamClient struct{ dirs chan string }

func (c wikiStreamClient) Complete(context.Context, string, string) (string, error) {
	return `{"answer":"# Architecture\nSee [[Architecture]]","results":[]}`, nil
}
func (c wikiStreamClient) CompleteInDir(ctx context.Context, dir, system, prompt string) (string, error) {
	c.dirs <- dir
	return c.Complete(ctx, system, prompt)
}

func TestKnowledgeAISearchHTTPStreamReceivesPayload(t *testing.T) {
	dir, project := t.TempDir(), t.TempDir()
	if err := indexPage(t, dir, "Architecture.md", "# Architecture\nShared architecture"); err != nil {
		t.Fatal(err)
	}
	client := wikiStreamClient{dirs: make(chan string, 1)}
	h := &WikiHandler{aiClient: client}
	server := httptest.NewServer(corsJSON(h.handleAISearch))
	defer server.Close()
	body, _ := json.Marshal(map[string]string{"dir": dir, "query": "architecture", "project_dir": project})
	req, _ := http.NewRequest("POST", server.URL, strings.NewReader(string(body)))
	req.Header.Set("Accept", "text/event-stream")
	req.Header.Set("Content-Type", "application/json")
	res, err := server.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	raw, err := io.ReadAll(res.Body)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"event: final", "# Architecture", "[[Architecture]]", "event: done"} {
		if !strings.Contains(string(raw), want) {
			t.Fatalf("missing %s: %s", want, raw)
		}
	}
	select {
	case got := <-client.dirs:
		if got != project {
			t.Fatalf("workdir=%s want=%s", got, project)
		}
	default:
		t.Fatal("agent was not called")
	}
	if strings.Contains(string(raw), "event: error") {
		t.Fatal(string(raw))
	}
}
