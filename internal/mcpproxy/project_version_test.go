package mcpproxy

import (
	"context"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/modelcontextprotocol/go-sdk/jsonrpc"
)

type versionTestConnection struct{ reads, writes chan jsonrpc.Message }

func (c *versionTestConnection) Read(ctx context.Context) (jsonrpc.Message, error) {
	select {
	case m := <-c.reads:
		return m, nil
	case <-ctx.Done():
		return nil, io.EOF
	}
}
func (c *versionTestConnection) Write(ctx context.Context, m jsonrpc.Message) error {
	select {
	case c.writes <- m:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}
func (*versionTestConnection) Close() error      { return nil }
func (*versionTestConnection) SessionID() string { return "fixture" }

func TestDreamProxyCarriesCapturedVersionAcrossRelayAndBranchChange(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	t.Setenv(projectversion.EnvVersion(), "before")
	if err := os.Mkdir(filepath.Join(dir, ".git"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".git", "HEAD"), []byte("ref: refs/heads/before\n"), 0600); err != nil {
		t.Fatal(err)
	}
	cfg := Config{CapabilityProfile: agentpolicy.ProfileDreamMemory}
	cfg.applyDefaults()
	if err := os.WriteFile(filepath.Join(dir, ".git", "HEAD"), []byte("ref: refs/heads/after\n"), 0600); err != nil {
		t.Fatal(err)
	}
	stdio := &versionTestConnection{reads: make(chan jsonrpc.Message, 1), writes: make(chan jsonrpc.Message, 1)}
	http := &versionTestConnection{reads: make(chan jsonrpc.Message, 1), writes: make(chan jsonrpc.Message, 1)}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- relay(ctx, stdio, http, cfg) }()
	args, _ := json.Marshal(map[string]any{"name": "graphit_memory_list", "arguments": map[string]any{"project_dir": dir, "top_k": 5}})
	original := &jsonrpc.Request{Method: "tools/call", Params: args}
	stdio.reads <- original
	select {
	case forwarded := <-http.writes:
		var body struct {
			Arguments map[string]any `json:"arguments"`
		}
		if err := json.Unmarshal(forwarded.(*jsonrpc.Request).Params, &body); err != nil {
			t.Fatal(err)
		}
		if body.Arguments["project_version"] != "before" || body.Arguments["top_k"] != float64(5) {
			t.Fatalf("forwarded scope: %v", body.Arguments)
		}
		var raw map[string]any
		_ = json.Unmarshal(original.Params, &raw)
		if _, ok := raw["arguments"].(map[string]any)["project_version"]; ok {
			t.Fatal("mutated caller request")
		}
	case <-time.After(time.Second):
		t.Fatal("relay did not forward")
	}
	cancel()
	<-done
}

func TestDreamProxyVersionPreservesExplicitAndOtherScopes(t *testing.T) {
	dir := t.TempDir()
	cfg := Config{pinnedProject: dir, pinnedVersion: "pinned"}
	for _, args := range []map[string]any{{"project_dir": dir, "project_version": "explicit"}, {"project_dir": t.TempDir()}, {"scope": "user"}, {"project_dir": dir, "project_version": 42}} {
		params, _ := json.Marshal(map[string]any{"name": "graphit_memory_list", "arguments": args})
		req := &jsonrpc.Request{Method: "tools/call", Params: params}
		if cfg.pinToolVersion(req) != req {
			t.Fatalf("unexpected rewrite: %v", args)
		}
	}
	req := &jsonrpc.Request{Method: "initialize", Params: json.RawMessage(`{}`)}
	if cfg.pinToolVersion(req) != req {
		t.Fatal("handshake rewritten")
	}
}

func TestDreamProxyVersionAnchorsRelativeProjectBeforeDaemonResolvesIt(t *testing.T) {
	dir := t.TempDir()
	cfg := Config{pinnedProject: dir, pinnedVersion: "before"}
	req := &jsonrpc.Request{Method: "tools/call", Params: json.RawMessage(`{"name":"graphit_memory_list","arguments":{"project_dir":"."}}`)}
	forwarded := cfg.pinToolVersion(req).(*jsonrpc.Request)
	var params struct {
		Arguments struct {
			ProjectDir     string `json:"project_dir"`
			ProjectVersion string `json:"project_version"`
		} `json:"arguments"`
	}
	if err := json.Unmarshal(forwarded.Params, &params); err != nil {
		t.Fatal(err)
	}
	resolved, _ := filepath.EvalSymlinks(dir)
	if params.Arguments.ProjectDir != resolved || !filepath.IsAbs(params.Arguments.ProjectDir) || params.Arguments.ProjectVersion != "before" {
		t.Fatalf("scope crosses daemon CWD: %+v", params.Arguments)
	}
}
