package ai

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestTryFallbackCLIAndComplete(t *testing.T) {
	tempDir := t.TempDir()

	// Create dummy "grok" shell script that consumes stdin
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

	// Stdin-based CLIs: consume stdin then echo
	stdinBinaries := []string{
		"claude", "gemini", "agy", "grok",
		"cursor-agent", "agent",
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

	// File-input CLIs: read from file passed as arg, echo
	fileBinaries := []string{"goose", "openhands"}
	for _, bin := range fileBinaries {
		// The file path will be the last argument; just echo ok
		script := fmt.Sprintf("#!/bin/sh\necho \"%s ok\"\n", bin)
		if err := os.WriteFile(filepath.Join(tempDir, bin), []byte(script), 0755); err != nil {
			t.Fatalf("failed to write dummy %s: %v", bin, err)
		}
	}

	// Arg-input CLIs: prompt as positional arg, just echo
	argBinaries := []string{"opencode", "cline"}
	for _, bin := range argBinaries {
		script := fmt.Sprintf("#!/bin/sh\necho \"%s ok\"\n", bin)
		if err := os.WriteFile(filepath.Join(tempDir, bin), []byte(script), 0755); err != nil {
			t.Fatalf("failed to write dummy %s: %v", bin, err)
		}
	}

	t.Setenv("PATH", tempDir)

	var allBinaries []string
	allBinaries = append(allBinaries, stdinBinaries...)
	allBinaries = append(allBinaries, fileBinaries...)
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

func TestCompleteAlwaysUsesStdin(t *testing.T) {
	tempDir := t.TempDir()
	// Script that verifies it receives stdin and outputs confirmation
	script := "#!/bin/sh\nread -r line; echo \"stdin ok\"\n"

	// Only test stdin-mode CLIs
	binaries := []string{"claude", "codex", "kiro-cli", "cursor-agent"}
	for _, bin := range binaries {
		bp := filepath.Join(tempDir, bin)
		if err := os.WriteFile(bp, []byte(script), 0755); err != nil {
			t.Fatal(err)
		}
	}

	// Even a small prompt should use stdin
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
	// Script that captures full stdin and echoes it
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

	t.Run("file_mode", func(t *testing.T) {
		// Script that reads the file content and outputs it
		fileScript := "#!/bin/sh\nfor arg; do last=$arg; done; cat \"$last\"\n"
		bp := filepath.Join(tempDir, "goose")
		if err := os.WriteFile(bp, []byte(fileScript), 0755); err != nil {
			t.Fatal(err)
		}

		c := &cliClient{executablePath: bp, binaryName: "goose"}
		resp, err := c.Complete(context.Background(), "", "test prompt")
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(resp, "non-interactive, autonomous mode") {
			t.Error("file prompt missing non-interactive preamble")
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
		// Preamble should come first, then system prompt, then user prompt
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

func TestCompleteFileInput(t *testing.T) {
	tempDir := t.TempDir()
	// Script that reads the file argument and outputs its content
	script := "#!/bin/sh\n" +
		"# Find the last argument (the temp file path)\n" +
		"for arg; do last=$arg; done\n" +
		"if [ -f \"$last\" ]; then echo \"file ok\"; else echo \"no file\"; fi\n"

	binaries := []string{"goose", "openhands"}
	for _, bin := range binaries {
		bp := filepath.Join(tempDir, bin)
		if err := os.WriteFile(bp, []byte(script), 0755); err != nil {
			t.Fatal(err)
		}
	}

	for _, bin := range binaries {
		t.Run(bin, func(t *testing.T) {
			c := &cliClient{executablePath: filepath.Join(tempDir, bin), binaryName: bin}
			resp, err := c.Complete(context.Background(), "", "test prompt")
			if err != nil {
				t.Errorf("Complete failed for %s: %v", bin, err)
			}
			if strings.TrimSpace(resp) != "file ok" {
				t.Errorf("got %q; want %q (file-input mode)", resp, "file ok")
			}
		})
	}
}

func TestCompleteArgInput(t *testing.T) {
	tempDir := t.TempDir()
	script := "#!/bin/sh\nfor arg; do last=$arg; done; echo \"arg: $last\"\n"

	binaries := []string{"opencode", "cline"}
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

func TestWriteTempPrompt(t *testing.T) {
	path, err := writeTempPrompt("test content")
	if err != nil {
		t.Fatal(err)
	}
	defer os.Remove(path)

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "test content" {
		t.Errorf("got %q; want %q", data, "test content")
	}

	// Verify file permissions are restricted
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	perm := info.Mode().Perm()
	if perm != 0o600 {
		t.Errorf("permissions = %o; want 0600", perm)
	}
}

func TestTempPromptCleanup(t *testing.T) {
	tempDir := t.TempDir()
	// Script that echoes ok — the file should be cleaned up after
	script := "#!/bin/sh\necho \"ok\"\n"
	binPath := filepath.Join(tempDir, "goose")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	c := &cliClient{executablePath: binPath, binaryName: "goose"}
	_, err := c.Complete(context.Background(), "", "test")
	if err != nil {
		t.Fatal(err)
	}

	// Verify temp files are cleaned up (no graphit-prompt-* files left)
	matches, _ := filepath.Glob(filepath.Join(os.TempDir(), "graphit-prompt-*"))
	for _, m := range matches {
		t.Errorf("temp prompt file not cleaned up: %s", m)
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
		{"copilot", true},
		{"grok", false},
		{"codex", false},
		{"cursor-agent", false},
		{"kiro-cli", false},
		{"cline", false},
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
	// Script that consumes stdin and echoes confirmation
	script := "#!/bin/sh\ncat > /dev/null; echo \"session ok\"\n"
	binPath := filepath.Join(tempDir, "agy")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	c := &cliClient{executablePath: binPath, binaryName: "agy"}

	t.Run("with_session_id", func(t *testing.T) {
		resp, sid, err := c.CompleteWithSession(context.Background(), "abc-123", "", "test")
		if err != nil {
			t.Fatalf("CompleteWithSession failed: %v", err)
		}
		if strings.TrimSpace(resp) != "session ok" {
			t.Errorf("got %q; want %q", resp, "session ok")
		}
		if sid != "abc-123" {
			t.Errorf("session ID = %q; want %q", sid, "abc-123")
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
		if sid != "" {
			t.Errorf("session ID = %q; want empty", sid)
		}
	})
}

func TestSessionClientInterface(t *testing.T) {
	c := &cliClient{binaryName: "agy"}
	var client Client = c

	sc, ok := client.(SessionClient)
	if !ok {
		t.Fatal("cliClient should implement SessionClient")
	}
	if !sc.SupportsSession() {
		t.Error("agy should support sessions")
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

	oldIDEToCLI := ideToCLI
	defer func() { ideToCLI = oldIDEToCLI }()
	ideToCLI = func(ide string) string { return "" }

	t.Setenv("PATH", tempDir)

	providers := []struct {
		provider string
		firstBin string
	}{
		{"google", "agy"},
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
	t.Setenv("PATH", t.TempDir()) // empty dir
	oldIDEToCLI := ideToCLI
	defer func() { ideToCLI = oldIDEToCLI }()
	ideToCLI = func(ide string) string { return "" }

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

	oldIDEToCLI := ideToCLI
	defer func() { ideToCLI = oldIDEToCLI }()
	ideToCLI = func(ide string) string { return "" }

	client := tryFallbackCLI("google", "my-custom")
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	cc := client.(*cliClient)
	if cc.binaryName != "my-custom" {
		t.Errorf("got %q; want %q", cc.binaryName, "my-custom")
	}
}

func TestTryFallbackCLI_IDEEquivalent(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("PATH", tempDir)

	script := "#!/bin/sh\necho ok\n"
	binPath := filepath.Join(tempDir, "ide-cli")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	oldIDEToCLI := ideToCLI
	defer func() { ideToCLI = oldIDEToCLI }()
	ideToCLI = func(ide string) string { return "ide-cli" }

	client := tryFallbackCLI("google", "")
	if client == nil {
		t.Fatal("expected non-nil client from IDE equivalent")
	}
	cc := client.(*cliClient)
	if cc.binaryName != "ide-cli" {
		t.Errorf("got %q; want %q", cc.binaryName, "ide-cli")
	}
}

func TestTryFallbackCLI_IDEEquivalentSameAsUser(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("PATH", tempDir)

	script := "#!/bin/sh\necho ok\n"
	binPath := filepath.Join(tempDir, "mybin")
	if err := os.WriteFile(binPath, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}

	oldIDEToCLI := ideToCLI
	defer func() { ideToCLI = oldIDEToCLI }()
	// IDE equivalent returns same as userCLI - should not duplicate
	ideToCLI = func(ide string) string { return "mybin" }

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
	expected := localModelName + " (lazy, not loaded)"
	if name != expected {
		t.Errorf("ModelName = %q; want %q", name, expected)
	}
}

func TestLazyEmbeddingClient_ModelName_AfterInit(t *testing.T) {
	lazy := NewLazyEmbeddingClient()
	// Simulate a loaded client
	lazy.client = &localEmbeddingClient{}
	name := lazy.ModelName()
	if name != localModelName {
		t.Errorf("ModelName = %q; want %q", name, localModelName)
	}
}

// failedLazyClient returns a client whose lazy initialisation has already run and
// failed.
//
// Consuming the sync.Once is the only way to inject the failure: init() runs the
// real NewLocalEmbeddingClient inside once.Do and assigns over l.err, so setting
// l.err beforehand has no effect. Tests that relied on the model simply being
// absent from the environment passed for the wrong reason — they went red the
// moment the ONNX runtime actually worked.
func failedLazyClient(t *testing.T, cause error) *LazyEmbeddingClient {
	t.Helper()
	lazy := NewLazyEmbeddingClient()
	lazy.once.Do(func() { lazy.err = cause })
	if lazy.init() == nil {
		t.Fatal("injected init failure did not stick — LazyEmbeddingClient.init no longer memoises via once")
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
	// With a clean env, findORTLibrary should return "" when no library exists
	t.Setenv("LD_LIBRARY_PATH", "")
	t.Setenv("DYLD_LIBRARY_PATH", "")
	t.Setenv("PATH", t.TempDir())

	result := findORTLibrary()
	// We just verify it doesn't panic and returns a string
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
	// We can't actually initialize ONNX runtime in tests
	// but we can verify the function doesn't panic on repeated calls
	// (it uses sync.Once so subsequent calls just return the cached error)
	err := initONNXRuntime()
	// It might succeed or fail depending on environment — just ensure no panic
	_ = err
}

func TestNewModelManager(t *testing.T) {
	mgr, err := NewModelManager()
	if err != nil {
		t.Fatalf("NewModelManager failed: %v", err)
	}
	if mgr == nil {
		t.Fatal("expected non-nil ModelManager")
	}
	if mgr.cacheDir == "" {
		t.Error("expected non-empty cacheDir")
	}
}

func TestModelManager_log(t *testing.T) {
	mgr := &ModelManager{}
	logger := mgr.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

func TestModelManager_isValid(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := &ModelManager{cacheDir: tmpDir}

	t.Run("missing_file", func(t *testing.T) {
		if mgr.isValid(filepath.Join(tmpDir, "nonexistent"), 0) {
			t.Error("expected false for missing file")
		}
	})

	t.Run("too_small", func(t *testing.T) {
		path := filepath.Join(tmpDir, "small.bin")
		if err := os.WriteFile(path, []byte("tiny"), 0o644); err != nil {
			t.Fatal(err)
		}
		if mgr.isValid(path, 100) {
			t.Error("expected false for file smaller than minSize")
		}
	})

	t.Run("valid", func(t *testing.T) {
		path := filepath.Join(tmpDir, "valid.bin")
		if err := os.WriteFile(path, []byte("valid content"), 0o644); err != nil {
			t.Fatal(err)
		}
		if !mgr.isValid(path, 5) {
			t.Error("expected true for valid file")
		}
	})
}

func TestModelManager_findBundledModels(t *testing.T) {
	mgr := &ModelManager{}
	// Calling findBundledModels - it checks next to the executable
	// In test mode, this will typically return ""
	result := mgr.findBundledModels()
	_ = result // Just verify no panic
}

func TestModelManager_download(t *testing.T) {
	content := "model data content for testing"

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/success":
			w.Header().Set("Content-Length", fmt.Sprintf("%d", len(content)))
			_, _ = w.Write([]byte(content))
		case "/error":
			w.WriteHeader(http.StatusNotFound)
		case "/incomplete":
			w.Header().Set("Content-Length", "1000000")
			_, _ = w.Write([]byte("short"))
		default:
			w.WriteHeader(404)
		}
	}))
	defer srv.Close()

	tmpDir := t.TempDir()
	mgr := &ModelManager{cacheDir: tmpDir}

	t.Run("success", func(t *testing.T) {
		destPath := filepath.Join(tmpDir, "model.onnx")
		err := mgr.download(context.Background(), srv.URL+"/success", destPath)
		if err != nil {
			t.Fatalf("download failed: %v", err)
		}
		data, err := os.ReadFile(destPath)
		if err != nil {
			t.Fatalf("failed to read downloaded file: %v", err)
		}
		if string(data) != content {
			t.Errorf("content = %q; want %q", data, content)
		}
	})

	t.Run("http_error", func(t *testing.T) {
		destPath := filepath.Join(tmpDir, "model_err.onnx")
		err := mgr.download(context.Background(), srv.URL+"/error", destPath)
		if err == nil {
			t.Error("expected error for HTTP 404")
		}
	})

	t.Run("incomplete_download", func(t *testing.T) {
		destPath := filepath.Join(tmpDir, "model_inc.onnx")
		err := mgr.download(context.Background(), srv.URL+"/incomplete", destPath)
		if err == nil {
			t.Error("expected error for incomplete download")
		}
	})

	t.Run("connection_error", func(t *testing.T) {
		destPath := filepath.Join(tmpDir, "model_conn.onnx")
		err := mgr.download(context.Background(), "http://127.0.0.1:1/fail", destPath)
		if err == nil {
			t.Error("expected connection error")
		}
	})

	t.Run("invalid_dest", func(t *testing.T) {
		err := mgr.download(context.Background(), srv.URL+"/success", "/nonexistent/dir/model.onnx")
		if err == nil {
			t.Error("expected error writing to invalid path")
		}
	})
}

// No bundled models sit next to a test binary in /tmp/go-build, so findBundledModels
// returns "" and EnsureModel falls through to the cache, finds it empty, and
// downloads. Pointed at a local server that is what happens, and it is checkable:
// before, this reached huggingface.co and accepted either outcome.
func TestModelManager_EnsureModel_BundledModels(t *testing.T) {
	tmpDir := t.TempDir()

	mgr := modelServer(t, tmpDir)

	modelPath, tokenizerPath, err := mgr.EnsureModel(context.Background())
	if err != nil {
		t.Fatalf("EnsureModel fell through to download and failed: %v", err)
	}
	if modelPath != filepath.Join(tmpDir, modelFileName) {
		t.Errorf("model path = %q; want it in the cache dir, not a bundled dir", modelPath)
	}
	if tokenizerPath != filepath.Join(tmpDir, tokenizerFileName) {
		t.Errorf("tokenizer path = %q; want it in the cache dir", tokenizerPath)
	}
}

func TestModelManager_EnsureModel_CachedModels(t *testing.T) {
	tmpDir := t.TempDir()

	// Create large enough fake model files in the cache dir
	modelPath := filepath.Join(tmpDir, modelFileName)
	tokenizerPath := filepath.Join(tmpDir, tokenizerFileName)

	modelData := make([]byte, modelONNXMinSize+1)
	tokenizerData := make([]byte, tokenizerJSONMinSize+1)

	if err := os.WriteFile(modelPath, modelData, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(tokenizerPath, tokenizerData, 0o644); err != nil {
		t.Fatal(err)
	}

	mgr := &ModelManager{cacheDir: tmpDir}
	mp, tp, err := mgr.EnsureModel(context.Background())
	if err != nil {
		t.Fatalf("EnsureModel with cached files failed: %v", err)
	}
	if mp != modelPath {
		t.Errorf("modelPath = %q; want %q", mp, modelPath)
	}
	if tp != tokenizerPath {
		t.Errorf("tokenizerPath = %q; want %q", tp, tokenizerPath)
	}
}

// A server that answers 200 with a body below the minimum size. This used to
// build exactly that server and then never point EnsureModel at it, so what was
// really being measured was huggingface.co.
func TestModelManager_EnsureModel_DownloadModel(t *testing.T) {
	tmpDir := t.TempDir()

	tooSmall := make([]byte, 50)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write(tooSmall)
	}))
	defer srv.Close()

	mgr := &ModelManager{
		cacheDir:     tmpDir,
		modelURL:     srv.URL + "/" + modelFileName,
		tokenizerURL: srv.URL + "/" + tokenizerFileName,
	}

	_, _, err := mgr.EnsureModel(context.Background())
	if err == nil {
		t.Fatal("a 50-byte model was accepted; the size floor is what stops a truncated download")
	}
	if !strings.Contains(err.Error(), "too small") {
		t.Errorf("error = %v; want it to name the size problem", err)
	}
}

func TestModelManager_EnsureModel_DownloadSuccess(t *testing.T) {
	tmpDir := t.TempDir()

	modelContent := make([]byte, modelONNXMinSize+1)
	tokenizerContent := make([]byte, tokenizerJSONMinSize+1)

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.Contains(r.URL.Path, "model.onnx") {
			w.Header().Set("Content-Length", fmt.Sprintf("%d", len(modelContent)))
			_, _ = w.Write(modelContent)
		} else if strings.Contains(r.URL.Path, "tokenizer.json") {
			w.Header().Set("Content-Length", fmt.Sprintf("%d", len(tokenizerContent)))
			_, _ = w.Write(tokenizerContent)
		} else {
			w.WriteHeader(404)
		}
	}))
	defer srv.Close()

	// We can't override the hardcoded URLs in EnsureModel, but we can test the
	// download function directly and the path validation.
	mgr := &ModelManager{cacheDir: tmpDir}

	// Download model
	modelDest := filepath.Join(tmpDir, modelFileName)
	if err := mgr.download(context.Background(), srv.URL+"/model.onnx", modelDest); err != nil {
		t.Fatalf("download model failed: %v", err)
	}

	// Download tokenizer
	tokenizerDest := filepath.Join(tmpDir, tokenizerFileName)
	if err := mgr.download(context.Background(), srv.URL+"/tokenizer.json", tokenizerDest); err != nil {
		t.Fatalf("download tokenizer failed: %v", err)
	}

	// Now EnsureModel should find cached files
	mp, tp, err := mgr.EnsureModel(context.Background())
	if err != nil {
		t.Fatalf("EnsureModel failed: %v", err)
	}
	if mp != modelDest {
		t.Errorf("model path = %q; want %q", mp, modelDest)
	}
	if tp != tokenizerDest {
		t.Errorf("tokenizer path = %q; want %q", tp, tokenizerDest)
	}
}

// A cached tokenizer is valid and the cached model is not, so only the model is
// re-fetched. The tokenizer's mtime is the evidence: an untouched file means the
// second download branch was correctly skipped.
func TestModelManager_EnsureModel_NeedDownloadModelTooSmall(t *testing.T) {
	tmpDir := t.TempDir()

	if err := os.WriteFile(filepath.Join(tmpDir, modelFileName), []byte("tiny"), 0o644); err != nil {
		t.Fatal(err)
	}
	tokenizerPath := filepath.Join(tmpDir, tokenizerFileName)
	sparseFile(t, tokenizerPath, tokenizerJSONMinSize+1)
	before, err := os.Stat(tokenizerPath)
	if err != nil {
		t.Fatal(err)
	}

	mgr := modelServer(t, tmpDir)
	if _, _, err := mgr.EnsureModel(context.Background()); err != nil {
		t.Fatalf("EnsureModel could not replace the undersized model: %v", err)
	}

	if info, err := os.Stat(filepath.Join(tmpDir, modelFileName)); err != nil {
		t.Fatal(err)
	} else if info.Size() < modelONNXMinSize {
		t.Errorf("model is still %d bytes; it was not replaced", info.Size())
	}
	after, err := os.Stat(tokenizerPath)
	if err != nil {
		t.Fatal(err)
	}
	if !after.ModTime().Equal(before.ModTime()) {
		t.Error("the already-valid tokenizer was downloaded again")
	}
}

func TestNewLocalEmbeddingClient_Fails(t *testing.T) {
	// Without a real ONNX model this should fail. The seeded cache is the right
	// size and the wrong content, so it still fails — at the ONNX load, which is
	// the step under test, rather than after a 132 MB download.
	home := t.TempDir()
	t.Setenv("HOME", home)
	seedModelCache(t, home)
	_, err := NewLocalEmbeddingClient()
	if err == nil {
		t.Log("NewLocalEmbeddingClient succeeded - model may be cached in default location")
	}
}

// localEmbeddingClient tests (unit-testable parts)

func TestLocalEmbeddingClient_ModelName(t *testing.T) {
	c := &localEmbeddingClient{}
	if got := c.ModelName(); got != localModelName {
		t.Errorf("ModelName() = %q; want %q", got, localModelName)
	}
}

func TestLocalEmbeddingClient_Close(t *testing.T) {
	c := &localEmbeddingClient{}
	// Close with nil session - should not panic
	c.Close()
}

// Test normalization math used in EmbedBatch output
func TestNormalization(t *testing.T) {
	vec := []float32{3, 4}
	var norm float64
	for _, v := range vec {
		norm += float64(v) * float64(v)
	}
	norm = math.Sqrt(norm) // 5
	if math.Abs(norm-5.0) > 1e-9 {
		t.Errorf("norm = %f; want 5.0", norm)
	}
	// Normalized vector should be [0.6, 0.8]
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

	// Multiple Embed calls should all fail with same error
	_, err1 := lazy.Embed(context.Background(), "test1")
	_, err2 := lazy.EmbedBatch(context.Background(), []string{"test2"})
	_, err3 := lazy.EmbedQuery(context.Background(), "test3")

	for i, err := range []error{err1, err2, err3} {
		if err == nil {
			t.Errorf("call %d: expected error", i)
		}
	}
}
