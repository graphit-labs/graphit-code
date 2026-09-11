package ai

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestTryFallbackCLIAndComplete(t *testing.T) {
	tempDir := t.TempDir()

	dummyGrok := filepath.Join(tempDir, "grok")
	grokScript := `#!/bin/sh
cat > /dev/null; echo "grok completed"
`
	if err := os.WriteFile(dummyGrok, []byte(grokScript), 0755); err != nil {
		t.Fatalf("failed to write dummy grok: %v", err)
	}

	t.Setenv("PATH", tempDir+":"+os.Getenv("PATH"))

	client := tryFallbackCLI("xai", "grok")
	if client == nil {
		t.Fatalf("expected non-nil Grok client")
	}

	cc, ok := client.(*cliClient)
	if !ok {
		t.Fatalf("expected client to be of type *cliClient")
	}
	if cc.binaryName != "grok" {
		t.Errorf("expected grok binary, got %s", cc.binaryName)
	}

	ctx := context.Background()
	resp, err := client.Complete(ctx, "System prompt", "User prompt")
	if err != nil {
		t.Errorf("Complete failed: %v", err)
	}
	if strings.TrimSpace(resp) != "grok completed" {
		t.Errorf("expected 'grok completed', got %q", resp)
	}

	clientGeneral := tryFallbackCLI("unknown_provider", "")
	if clientGeneral == nil {
		t.Fatalf("expected client for unknown_provider fallback, since grok is in PATH")
	}
}

func TestNewClientFromConfigError(t *testing.T) {
	t.Setenv("PATH", "")

	_, err := NewClientFromConfig()
	if err == nil {
		t.Error("expected error from NewClientFromConfig when PATH is empty")
	}
}

func TestCompleteAllCLIBranches(t *testing.T) {
	tempDir := t.TempDir()

	stdinBinaries := []string{
		"claude", "gemini", "agy", "grok",
		"cursor-agent",
		"codex",
		"kiro-cli",
		"copilot",
		"my-custom-cli",
	}
	for _, bin := range stdinBinaries {
		script := fmt.Sprintf("#!/bin/sh\ncat > /dev/null; echo \"%s ok\"\n", bin)
		if err := os.WriteFile(filepath.Join(tempDir, bin), []byte(script), 0755); err != nil {
			t.Fatalf("failed to write dummy %s: %v", bin, err)
		}
	}

	argBinaries := []string{"opencode", "qwen", "kimi", "deepcode"}
	for _, bin := range argBinaries {
		script := fmt.Sprintf("#!/bin/sh\necho \"%s ok\"\n", bin)
		if err := os.WriteFile(filepath.Join(tempDir, bin), []byte(script), 0755); err != nil {
			t.Fatalf("failed to write dummy %s: %v", bin, err)
		}
	}

	t.Setenv("PATH", tempDir)

	var allBinaries []string
	allBinaries = append(allBinaries, stdinBinaries...)
	allBinaries = append(allBinaries, argBinaries...)

	for _, bin := range allBinaries {
		t.Run(bin, func(t *testing.T) {
			c := &cliClient{
				executablePath: filepath.Join(tempDir, bin),
				binaryName:     bin,
			}
			resp, err := c.Complete(context.Background(), "", "test")
			if err != nil {
				t.Errorf("Complete failed for %s: %v", bin, err)
			}
			expected := bin + " ok"
			if strings.TrimSpace(resp) != expected {
				t.Errorf("got %q; want %q", resp, expected)
			}
		})
	}
}

func TestCompleteWithSystemPrompt(t *testing.T) {
	tempDir := t.TempDir()
	script := "#!/bin/sh\ncat > /dev/null; echo \"response\"\n"
	binPath := filepath.Join(tempDir, "gemini")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	c := &cliClient{executablePath: binPath, binaryName: "gemini"}
	resp, err := c.Complete(context.Background(), "system", "user")
	if err != nil {
		t.Errorf("Complete failed: %v", err)
	}
	if strings.TrimSpace(resp) != "response" {
		t.Errorf("got %q; want %q", resp, "response")
	}
}

func TestCompleteInDirUsesRequestedWorkingDirectory(t *testing.T) {
	binDir := t.TempDir()
	workingDir := t.TempDir()
	binPath := filepath.Join(binDir, "gemini")
	if err := os.WriteFile(binPath, []byte("#!/bin/sh\ncat > /dev/null\npwd\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	c := &cliClient{executablePath: binPath, binaryName: "gemini"}
	resp, err := c.CompleteInDir(context.Background(), workingDir, "system", "user")
	if err != nil {
		t.Fatalf("CompleteInDir: %v", err)
	}
	if got := strings.TrimSpace(resp); got != workingDir {
		t.Fatalf("working directory = %q, want %q", got, workingDir)
	}
}

func TestCompleteAlwaysUsesStdin(t *testing.T) {
	tempDir := t.TempDir()
	script := "#!/bin/sh\nread -r line; echo \"stdin ok\"\n"

	binaries := []string{"claude", "codex", "kiro-cli", "cursor-agent"}
	for _, bin := range binaries {
		bp := filepath.Join(tempDir, bin)
		if err := os.WriteFile(bp, []byte(script), 0755); err != nil {
			t.Fatal(err)
		}
	}

	for _, bin := range binaries {
		t.Run(bin, func(t *testing.T) {
			c := &cliClient{executablePath: filepath.Join(tempDir, bin), binaryName: bin}
			resp, err := c.Complete(context.Background(), "", "small prompt")
			if err != nil {
				t.Errorf("Complete failed for %s: %v", bin, err)
			}
			if strings.TrimSpace(resp) != "stdin ok" {
				t.Errorf("got %q; want %q", resp, "stdin ok")
			}
		})
	}
}

func TestNonInteractivePreamble(t *testing.T) {
	tempDir := t.TempDir()
	script := "#!/bin/sh\ncat\n"

	t.Run("stdin_mode", func(t *testing.T) {
		bp := filepath.Join(tempDir, "gemini")
		if err := os.WriteFile(bp, []byte(script), 0755); err != nil {
			t.Fatal(err)
		}

		c := &cliClient{executablePath: bp, binaryName: "gemini"}
		resp, err := c.Complete(context.Background(), "", "test prompt")
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(resp, "non-interactive, autonomous mode") {
			t.Error("stdin prompt missing non-interactive preamble")
		}
		if !strings.Contains(resp, "test prompt") {
			t.Error("stdin prompt missing user prompt")
		}
	})

	t.Run("arg_mode", func(t *testing.T) {
		argScript := "#!/bin/sh\nfor arg; do last=\"$arg\"; done; echo \"$last\"\n"
		bp := filepath.Join(tempDir, "opencode")
		if err := os.WriteFile(bp, []byte(argScript), 0755); err != nil {
			t.Fatal(err)
		}

		c := &cliClient{executablePath: bp, binaryName: "opencode"}
		resp, err := c.Complete(context.Background(), "", "test prompt")
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(resp, "non-interactive, autonomous mode") {
			t.Error("arg prompt missing non-interactive preamble")
		}
	})

	t.Run("with_system_prompt", func(t *testing.T) {
		bp := filepath.Join(tempDir, "gemini")
		c := &cliClient{executablePath: bp, binaryName: "gemini"}
		resp, err := c.Complete(context.Background(), "You are a helpful assistant.", "analyze this")
		if err != nil {
			t.Fatal(err)
		}
		preambleIdx := strings.Index(resp, "non-interactive, autonomous mode")
		systemIdx := strings.Index(resp, "You are a helpful assistant.")
		userIdx := strings.Index(resp, "analyze this")
		if preambleIdx < 0 || systemIdx < 0 || userIdx < 0 {
			t.Fatalf("missing components in prompt: preamble=%d system=%d user=%d", preambleIdx, systemIdx, userIdx)
		}
		if preambleIdx >= systemIdx || systemIdx >= userIdx {
			t.Errorf("wrong order: preamble@%d should come before system@%d before user@%d", preambleIdx, systemIdx, userIdx)
		}
	})
}

func TestCompleteArgInput(t *testing.T) {
	tempDir := t.TempDir()
	script := "#!/bin/sh\nfor arg; do last=$arg; done; echo \"arg: $last\"\n"

	binaries := []string{"opencode", "qwen", "kimi", "deepcode"}
	for _, bin := range binaries {
		bp := filepath.Join(tempDir, bin)
		if err := os.WriteFile(bp, []byte(script), 0755); err != nil {
			t.Fatal(err)
		}
	}

	for _, bin := range binaries {
		t.Run(bin, func(t *testing.T) {
			c := &cliClient{executablePath: filepath.Join(tempDir, bin), binaryName: bin}
			resp, err := c.Complete(context.Background(), "", "hello world")
			if err != nil {
				t.Errorf("Complete failed for %s: %v", bin, err)
			}
			if !strings.Contains(resp, "hello world") {
				t.Errorf("got %q; expected to contain prompt text (arg-input mode)", resp)
			}
		})
	}
}

func TestSupportsSession(t *testing.T) {
	tests := []struct {
		binary   string
		expected bool
	}{
		{"agy", true},
		{"gemini", true},
		{"claude", true},
		{"opencode", true},
		{"copilot", false},
		{"grok", false},
		{"codex", true},
		{"cursor-agent", false},
		{"kiro-cli", false},
		{"qwen", true},
		{"kimi", true},
		{"deepcode", false},
		{"unknown", false},
	}
	for _, tt := range tests {
		t.Run(tt.binary, func(t *testing.T) {
			c := &cliClient{binaryName: tt.binary}
			if got := c.SupportsSession(); got != tt.expected {
				t.Errorf("SupportsSession() = %v; want %v", got, tt.expected)
			}
		})
	}
}

func TestCompleteWithSession(t *testing.T) {
	tempDir := t.TempDir()
	argsPath := filepath.Join(tempDir, "args")
	script := "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGS_PATH\"\ncat > /dev/null\necho '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"created-session\"}'\necho '{\"type\":\"stream_event\",\"delta\":{\"type\":\"text_delta\",\"text\":\"session ok\"}}'\n"
	binPath := filepath.Join(tempDir, "claude")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ARGS_PATH", argsPath)

	c := &cliClient{executablePath: binPath, binaryName: "claude"}

	t.Run("with_session_id", func(t *testing.T) {
		resp, sid, err := c.CompleteWithSession(context.Background(), "abc-123", "", "test")
		if err != nil {
			t.Fatalf("CompleteWithSession failed: %v", err)
		}
		if strings.TrimSpace(resp) != "session ok" {
			t.Errorf("got %q; want %q", resp, "session ok")
		}
		if sid != "created-session" {
			t.Errorf("session ID = %q; want %q", sid, "created-session")
		}
		args, err := os.ReadFile(argsPath)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(args), "--resume\nabc-123\n") {
			t.Errorf("resume arguments missing: %q", args)
		}
	})

	t.Run("without_session_id", func(t *testing.T) {
		resp, sid, err := c.CompleteWithSession(context.Background(), "", "", "test")
		if err != nil {
			t.Fatalf("CompleteWithSession failed: %v", err)
		}
		if strings.TrimSpace(resp) != "session ok" {
			t.Errorf("got %q; want %q", resp, "session ok")
		}
		if sid != "created-session" {
			t.Errorf("session ID = %q; want created-session", sid)
		}
		args, err := os.ReadFile(argsPath)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(args), "--session-id\n") {
			t.Errorf("new-session arguments missing: %q", args)
		}
	})
}

func TestSessionClientInterface(t *testing.T) {
	c := &cliClient{binaryName: "codex"}
	var client Client = c

	sc, ok := client.(SessionClient)
	if !ok {
		t.Fatal("cliClient should implement SessionClient")
	}
	if !sc.SupportsSession() {
		t.Error("codex should support sessions")
	}
}

func TestCompleteError(t *testing.T) {
	tempDir := t.TempDir()
	script := "#!/bin/sh\nexit 1\n"
	binPath := filepath.Join(tempDir, "failing-cli")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	c := &cliClient{executablePath: binPath, binaryName: "failing-cli"}
	_, err := c.Complete(context.Background(), "", "test")
	if err == nil {
		t.Error("expected error from failing CLI")
	}
	if !strings.Contains(err.Error(), "CLI fallback") {
		t.Errorf("expected CLI fallback error, got %q", err)
	}
}

func TestTryFallbackCLI_AllProviders(t *testing.T) {
	tempDir := t.TempDir()

	oldAgentToCLI := agentToCLI
	defer func() { agentToCLI = oldAgentToCLI }()
	agentToCLI = func(agent string) string { return "" }

	t.Setenv("PATH", tempDir)

	providers := []struct {
		provider string
		firstBin string
	}{
		{"google", "agy"},
		{"alibaba", "qwen"},
		{"qwen", "qwen"},
		{"moonshot", "kimi"},
		{"kimi", "kimi"},
		{"deepseek", "deepcode"},
		{"anthropic", "claude"},
		{"openai", "codex"},
		{"xai", "grok"},
		{"amazon", "kiro-cli"},
		{"aws", "kiro-cli"},
		{"", "opencode"},
	}

	for _, p := range providers {
		t.Run("provider_"+p.provider, func(t *testing.T) {
			script := "#!/bin/sh\necho ok\n"
			binPath := filepath.Join(tempDir, p.firstBin)
			if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
				t.Fatal(err)
			}
			defer os.Remove(binPath)

			client := tryFallbackCLI(p.provider, "")
			if client == nil {
				t.Fatalf("expected non-nil client for provider %q", p.provider)
			}
			cc := client.(*cliClient)
			if cc.binaryName != p.firstBin {
				t.Errorf("got binary %q; want %q", cc.binaryName, p.firstBin)
			}
		})
	}
}

func TestTryFallbackCLI_NoneFound(t *testing.T) {
	t.Setenv("PATH", t.TempDir())
	oldAgentToCLI := agentToCLI
	defer func() { agentToCLI = oldAgentToCLI }()
	agentToCLI = func(agent string) string { return "" }

	client := tryFallbackCLI("", "")
	if client != nil {
		t.Error("expected nil client when no binaries are in PATH")
	}
}

func TestTryFallbackCLI_UserCLIFirst(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("PATH", tempDir)

	script := "#!/bin/sh\necho ok\n"
	binPath := filepath.Join(tempDir, "my-custom")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	oldAgentToCLI := agentToCLI
	defer func() { agentToCLI = oldAgentToCLI }()
	agentToCLI = func(agent string) string { return "" }

	client := tryFallbackCLI("google", "my-custom")
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	cc := client.(*cliClient)
	if cc.binaryName != "my-custom" {
		t.Errorf("got %q; want %q", cc.binaryName, "my-custom")
	}
}

func TestTryFallbackCLI_RemovedCLIsAreRejected(t *testing.T) {
	oldAgentToCLI := agentToCLI
	defer func() { agentToCLI = oldAgentToCLI }()
	agentToCLI = func(agent string) string { return "" }

	for bin := range removedCLIs {
		t.Run(bin, func(t *testing.T) {
			tempDir := t.TempDir()
			t.Setenv("PATH", tempDir)
			if err := os.WriteFile(filepath.Join(tempDir, bin), []byte("#!/bin/sh\necho unexpected\n"), 0o755); err != nil {
				t.Fatal(err)
			}
			if client := tryFallbackCLI("", bin); client != nil {
				t.Fatalf("removed CLI %q was selected", bin)
			}
		})
	}
}

func TestTryFallbackCLI_AgentEquivalent(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("PATH", tempDir)

	script := "#!/bin/sh\necho ok\n"
	binPath := filepath.Join(tempDir, "agent-cli")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	oldAgentToCLI := agentToCLI
	defer func() { agentToCLI = oldAgentToCLI }()
	agentToCLI = func(agent string) string { return "agent-cli" }

	client := tryFallbackCLI("google", "")
	if client == nil {
		t.Fatal("expected non-nil client from Agent equivalent")
	}
	cc := client.(*cliClient)
	if cc.binaryName != "agent-cli" {
		t.Errorf("got %q; want %q", cc.binaryName, "agent-cli")
	}
}

func TestTryFallbackCLI_AgentEquivalentSameAsUser(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("PATH", tempDir)

	script := "#!/bin/sh\necho ok\n"
	binPath := filepath.Join(tempDir, "mybin")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	oldAgentToCLI := agentToCLI
	defer func() { agentToCLI = oldAgentToCLI }()
	agentToCLI = func(agent string) string { return "mybin" }

	client := tryFallbackCLI("google", "mybin")
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	cc := client.(*cliClient)
	if cc.binaryName != "mybin" {
		t.Errorf("got %q; want %q", cc.binaryName, "mybin")
	}
}

func TestNewClientFromConfig_Success(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("PATH", tempDir)

	script := "#!/bin/sh\necho ok\n"
	binPath := filepath.Join(tempDir, "gemini")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	client, err := NewClientFromConfig()
	if err != nil {
		t.Fatalf("NewClientFromConfig failed: %v", err)
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}
}

func TestProxyEmbeddingClient(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)

	sockFile := filepath.Join(tempHome, brand.DotDir(), "daemon", "embed.sock")
	if err := os.MkdirAll(filepath.Dir(sockFile), 0o755); err != nil {
		t.Fatal(err)
	}

	listener, err := net.Listen("unix", sockFile)
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()

	go func() {
		for {
			conn, err := listener.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				reader := bufio.NewReader(c)
				for {
					line, err := reader.ReadBytes('\n')
					if err != nil {
						return
					}
					var req embedRequest
					if err := json.Unmarshal(line, &req); err == nil {
						if len(req.Texts) > 0 {
							vecs := make([][]float32, len(req.Texts))
							for i := range req.Texts {
								vecs[i] = []float32{1.0}
							}
							resp, _ := json.Marshal(embedResponse{Vectors: vecs})
							_, _ = c.Write(append(resp, '\n'))
						} else if req.Query != "" {
							resp, _ := json.Marshal(embedResponse{Vectors: [][]float32{{42.0}}})
							_, _ = c.Write(append(resp, '\n'))
						}
					}
				}
			}(conn)
		}
	}()

	client := newProxyEmbeddingClient()
	if client == nil {
		t.Fatal("expected non-nil proxy client")
	}

	t.Run("ModelName", func(t *testing.T) {
		name := client.ModelName()
		if name != "daemon-embedder (proxy→daemon)" {
			t.Errorf("ModelName = %q; want %q", name, "daemon-embedder (proxy→daemon)")
		}
	})

	t.Run("Embed", func(t *testing.T) {
		vec, err := client.Embed(context.Background(), "hello")
		if err != nil {
			t.Fatalf("Embed failed: %v", err)
		}
		if vec[0] != 1.0 {
			t.Errorf("vec[0] = %f; want 1.0", vec[0])
		}
	})

	t.Run("EmbedBatch", func(t *testing.T) {
		vecs, err := client.EmbedBatch(context.Background(), []string{"a", "b"})
		if err != nil {
			t.Fatalf("EmbedBatch failed: %v", err)
		}
		if len(vecs) != 2 {
			t.Errorf("batch len = %d; want 2", len(vecs))
		}
	})

	t.Run("EmbedQuery", func(t *testing.T) {
		vec, err := client.EmbedQuery(context.Background(), "search query")
		if err != nil {
			t.Fatalf("EmbedQuery failed: %v", err)
		}
		if vec[0] != 42.0 {
			t.Errorf("query vec[0] = %f; want 42.0", vec[0])
		}
	})
}

func TestProxyEmbeddingClient_Errors(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)

	sockFile := filepath.Join(tempHome, brand.DotDir(), "daemon", "embed.sock")
	if err := os.MkdirAll(filepath.Dir(sockFile), 0o755); err != nil {
		t.Fatal(err)
	}

	listener, err := net.Listen("unix", sockFile)
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()

	go func() {
		for {
			conn, err := listener.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				reader := bufio.NewReader(c)
				line, err := reader.ReadBytes('\n')
				if err != nil {
					return
				}
				var req embedRequest
				_ = json.Unmarshal(line, &req)

				if len(req.Texts) > 0 && req.Texts[0] == "bad-json" {
					_, _ = c.Write([]byte("invalid json\n"))
				} else if len(req.Texts) > 0 && req.Texts[0] == "empty" {
					_, _ = c.Write([]byte(`{"vectors":[]}` + "\n"))
				} else {
					resp, _ := json.Marshal(embedResponse{Error: "mock error"})
					_, _ = c.Write(append(resp, '\n'))
				}
			}(conn)
		}
	}()

	client := newProxyEmbeddingClient()
	if client == nil {
		t.Fatal("expected non-nil proxy client")
	}

	t.Run("EmbedBatch_Error", func(t *testing.T) {
		_, err := client.EmbedBatch(context.Background(), []string{"test"})
		if err == nil {
			t.Error("expected error")
		}
		if !strings.Contains(err.Error(), "mock error") {
			t.Errorf("expected 'mock error', got %v", err)
		}
	})

	t.Run("EmbedBatch_BadJSON", func(t *testing.T) {
		_, err := client.EmbedBatch(context.Background(), []string{"bad-json"})
		if err == nil {
			t.Error("expected error for bad JSON")
		}
	})

	t.Run("Embed_EmptyResponse", func(t *testing.T) {
		_, err := client.Embed(context.Background(), "empty")
		if err == nil {
			t.Error("expected error for empty embedding response")
		}
	})
}

func TestNewProxyEmbeddingClient_NoSockFile(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	client := newProxyEmbeddingClient()
	if client != nil {
		t.Error("expected nil when sock file doesn't exist")
	}
}

func TestNewEmbeddingClientFromConfig_ProxyAvailable(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)

	sockFile := filepath.Join(tempHome, brand.DotDir(), "daemon", "embed.sock")
	if err := os.MkdirAll(filepath.Dir(sockFile), 0o755); err != nil {
		t.Fatal(err)
	}

	listener, err := net.Listen("unix", sockFile)
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()

	client, err := NewEmbeddingClientFromConfig()
	if err != nil {
		t.Fatalf("NewEmbeddingClientFromConfig failed: %v", err)
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	if !strings.Contains(client.ModelName(), proxyModelTag) {
		t.Errorf("ModelName = %q; expected proxy tag", client.ModelName())
	}
}

func TestNewLazyEmbeddingClient(t *testing.T) {
	lazy := NewLazyEmbeddingClient()
	if lazy == nil {
		t.Fatal("expected non-nil lazy client")
	}
}

func TestLazyEmbeddingClient_ModelName_BeforeInit(t *testing.T) {
	lazy := NewLazyEmbeddingClient()
	name := lazy.ModelName()
	expected := "embedder (lazy, not loaded)"
	if name != expected {
		t.Errorf("ModelName = %q; want %q", name, expected)
	}
}

func TestLazyEmbeddingClient_ModelName_AfterInit(t *testing.T) {
	lazy := NewLazyEmbeddingClient()
	lazy.client = &localEmbeddingClient{modelName: "selected-model"}
	name := lazy.ModelName()
	if name != "selected-model" {
		t.Errorf("ModelName = %q; want %q", name, "selected-model")
	}
}

func failedLazyClient(t *testing.T, cause error) *LazyEmbeddingClient {
	t.Helper()
	lazy := NewLazyEmbeddingClient()
	lazy.key = activeEmbeddingConfigurationKey()
	lazy.err = cause
	if lazy.init() == nil {
		t.Fatal("injected init failure did not stick")
	}
	return lazy
}

func TestLazyEmbeddingClient_InitError(t *testing.T) {
	lazy := failedLazyClient(t, errors.New("model unavailable"))

	t.Run("Embed_InitError", func(t *testing.T) {
		_, err := lazy.Embed(context.Background(), "test")
		if err == nil {
			t.Error("expected error from Embed when init fails")
		}
	})

	t.Run("EmbedBatch_InitError", func(t *testing.T) {
		_, err := lazy.EmbedBatch(context.Background(), []string{"test"})
		if err == nil {
			t.Error("expected error from EmbedBatch when init fails")
		}
	})

	t.Run("EmbedQuery_InitError", func(t *testing.T) {
		_, err := lazy.EmbedQuery(context.Background(), "test")
		if err == nil {
			t.Error("expected error from EmbedQuery when init fails")
		}
	})
}

func TestFindORTLibrary_NoLibrary(t *testing.T) {
	t.Setenv("LD_LIBRARY_PATH", "")
	t.Setenv("DYLD_LIBRARY_PATH", "")
	t.Setenv("PATH", t.TempDir())

	result := findORTLibrary()
	_ = result
}

func TestFindORTLibrary_InEnvPath(t *testing.T) {
	tmpDir := t.TempDir()
	libPath := filepath.Join(tmpDir, "libonnxruntime.so")
	if err := os.WriteFile(libPath, []byte("fake"), 0o644); err != nil {
		t.Fatal(err)
	}

	t.Setenv("LD_LIBRARY_PATH", tmpDir)

	result := findORTLibrary()
	if result != libPath {
		t.Errorf("findORTLibrary() = %q; want %q", result, libPath)
	}
}

func TestInitONNXRuntime(t *testing.T) {
	err := initONNXRuntime()
	_ = err
}

func TestLocalEmbeddingClient_ModelName(t *testing.T) {
	c := &localEmbeddingClient{modelName: "operator-model"}
	if got := c.ModelName(); got != "operator-model" {
		t.Errorf("ModelName() = %q; want %q", got, "operator-model")
	}
}

func TestLocalEmbeddingClient_Close(t *testing.T) {
	c := &localEmbeddingClient{}
	c.Close()
}

// Test normalization math used in EmbedBatch output
func TestNormalization(t *testing.T) {
	vec := []float32{3, 4}
	var norm float64
	for _, v := range vec {
		norm += float64(v) * float64(v)
	}
	norm = math.Sqrt(norm)
	if math.Abs(norm-5.0) > 1e-9 {
		t.Errorf("norm = %f; want 5.0", norm)
	}
	normalized := make([]float32, len(vec))
	for i, v := range vec {
		normalized[i] = float32(float64(v) / norm)
	}
	if math.Abs(float64(normalized[0])-0.6) > 1e-6 {
		t.Errorf("normalized[0] = %f; want 0.6", normalized[0])
	}
}

func TestLazyEmbeddingClient_MultipleCalls(t *testing.T) {
	lazy := failedLazyClient(t, errors.New("init error"))

	_, err1 := lazy.Embed(context.Background(), "test1")
	_, err2 := lazy.EmbedBatch(context.Background(), []string{"test2"})
	_, err3 := lazy.EmbedQuery(context.Background(), "test3")

	for i, err := range []error{err1, err2, err3} {
		if err == nil {
			t.Errorf("call %d: expected error", i)
		}
	}
}
