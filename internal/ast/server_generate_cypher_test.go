package ast

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestGenerationRepoPathPrefersRequestedProject(t *testing.T) {
	t.Parallel()

	if got := generationRepoPath("/projects/linux", "/projects/graphit-code"); got != "/projects/linux" {
		t.Fatalf("generationRepoPath() = %q, want requested project", got)
	}
	if got := generationRepoPath("", "/projects/graphit-code"); got != "/projects/graphit-code" {
		t.Fatalf("generationRepoPath() fallback = %q, want server repository", got)
	}
}

func TestGenerateCypherHTTPStreamReceivesPayload(t *testing.T) {
	previous := generateFunc
	t.Cleanup(func() { generateFunc = previous })
	requests := make(chan AICypherRequest, 1)
	RegisterAICypherGenerator(func(ctx context.Context, db GraphDB, client AIClient, req AICypherRequest) (*AICypherResponse, error) {
		requests <- req
		return &AICypherResponse{Cypher: "MATCH (n:Function) RETURN n.name LIMIT 5"}, nil
	})
	serverDir, selected := t.TempDir(), t.TempDir()
	s := &Server{db: &emptyGraphDB{}, aiClient: cypherStreamClient{}, repoPath: serverDir}
	server := httptest.NewServer(http.HandlerFunc(s.handleGenerateCypher))
	defer server.Close()
	body, _ := json.Marshal(map[string]string{"query": "find functions", "project_dir": selected})
	req, _ := http.NewRequest("POST", server.URL, strings.NewReader(string(body)))
	req.Header.Set("Accept", "text/event-stream")
	res, err := server.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	raw, err := io.ReadAll(res.Body)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"event: final", "MATCH (n:Function)", `"original_query":"find functions"`, "event: done"} {
		if !strings.Contains(string(raw), want) {
			t.Fatalf("missing %s: %s", want, raw)
		}
	}
	select {
	case got := <-requests:
		if got.RepoPath != selected || got.Execute || got.UserQuery != "find functions" || got.MaxResults != 25 {
			t.Fatalf("request=%+v", got)
		}
	default:
		t.Fatal("generator was not called")
	}
}

type cypherStreamClient struct{}

func (cypherStreamClient) Complete(context.Context, string, string) (string, error) {
	return "unused", nil
}
