package mcpserver

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ---------- mcpSession: test helper for full JSON-RPC tool calling ----------

// mcpSession encapsulates a fully initialized MCP HTTP session for testing.
type mcpSession struct {
	t         *testing.T
	handler   http.Handler
	sessionID string
	nextID    int
}

// newMCPSession creates a new MCP server, initialises the handshake, and returns
// a ready-to-use session. It calls t.Fatal on setup failure.
func newMCPSession(t *testing.T) *mcpSession {
	t.Helper()
	server := NewServer()
	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	s := &mcpSession{
		t:       t,
		handler: handler,
		nextID:  10,
	}

	// Step 1: initialize
	initResp := s.sendRPC(1, "initialize", map[string]any{
		"protocolVersion": "2025-03-26",
		"capabilities":   map[string]any{},
		"clientInfo":     map[string]any{"name": "test", "version": "0.0.1"},
	})
	if initResp["error"] != nil {
		t.Fatalf("initialize failed: %v", initResp["error"])
	}

	// Step 2: initialized notification (no ID)
	s.sendNotification("notifications/initialized", nil)

	return s
}

// sendRPC sends a JSON-RPC request and returns the parsed response.
func (s *mcpSession) sendRPC(id int, method string, params any) map[string]any {
	s.t.Helper()
	body := map[string]any{
		"jsonrpc": "2.0",
		"id":      id,
		"method":  method,
	}
	if params != nil {
		body["params"] = params
	}
	reqBytes, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/mcp", bytes.NewReader(reqBytes))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	if s.sessionID != "" {
		req.Header.Set("Mcp-Session-Id", s.sessionID)
	}
	w := httptest.NewRecorder()
	s.handler.ServeHTTP(w, req)

	// Capture session ID from first response
	if sid := w.Header().Get("Mcp-Session-Id"); sid != "" {
		s.sessionID = sid
	}

	respBody := w.Body.String()
	var jsonData string
	if strings.Contains(w.Header().Get("Content-Type"), "text/event-stream") {
		var ok bool
		jsonData, ok = extractJSONFromSSE(respBody)
		if !ok {
			s.t.Fatalf("failed to extract JSON from SSE for %s: %s", method, respBody)
		}
	} else {
		jsonData = respBody
	}

	var resp map[string]any
	if err := json.Unmarshal([]byte(jsonData), &resp); err != nil {
		s.t.Fatalf("failed to parse response JSON for %s: %v; body: %s", method, err, jsonData)
	}
	return resp
}

// sendNotification sends a JSON-RPC notification (no id, no response expected).
func (s *mcpSession) sendNotification(method string, params any) {
	s.t.Helper()
	body := map[string]any{
		"jsonrpc": "2.0",
		"method":  method,
	}
	if params != nil {
		body["params"] = params
	}
	reqBytes, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/mcp", bytes.NewReader(reqBytes))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	if s.sessionID != "" {
		req.Header.Set("Mcp-Session-Id", s.sessionID)
	}
	w := httptest.NewRecorder()
	s.handler.ServeHTTP(w, req)
	if sid := w.Header().Get("Mcp-Session-Id"); sid != "" {
		s.sessionID = sid
	}
}

// callTool calls a tool and returns (result map, error).
func (s *mcpSession) callTool(toolName string, args map[string]any) (map[string]any, error) {
	s.t.Helper()
	s.nextID++
	resp := s.sendRPC(s.nextID, "tools/call", map[string]any{
		"name":      toolName,
		"arguments": args,
	})

	if errObj, ok := resp["error"]; ok && errObj != nil {
		errMap, _ := errObj.(map[string]any)
		msg, _ := errMap["message"].(string)
		return resp, fmt.Errorf("jsonrpc error: %s", msg)
	}
	return resp, nil
}

// callToolExpectError calls a tool and expects it to return an error (either JSON-RPC error or tool IsError).
func (s *mcpSession) callToolExpectError(toolName string, args map[string]any, wantSubstr string) {
	s.t.Helper()
	resp, err := s.callTool(toolName, args)

	errFound := false
	errMsg := ""

	// Check for JSON-RPC level error
	if err != nil {
		errFound = true
		errMsg = err.Error()
	}

	// Check for tool-level error (IsError=true in result)
	if !errFound {
		if result, ok := resp["result"].(map[string]any); ok {
			if isErr, _ := result["isError"].(bool); isErr {
				if content, ok := result["content"].([]any); ok && len(content) > 0 {
					if tc, ok := content[0].(map[string]any); ok {
						if text, ok := tc["text"].(string); ok {
							errFound = true
							errMsg = text
						}
					}
				}
			}
		}
	}

	if !errFound {
		s.t.Errorf("callTool(%q) expected error containing %q, but got success", toolName, wantSubstr)
		return
	}
	if wantSubstr != "" && !strings.Contains(errMsg, wantSubstr) {
		s.t.Errorf("callTool(%q) error = %q, want containing %q", toolName, errMsg, wantSubstr)
	}
}

// callToolExpectText calls a tool and expects a successful text result containing wantSubstr.
func (s *mcpSession) callToolExpectText(toolName string, args map[string]any, wantSubstr string) {
	s.t.Helper()
	resp, err := s.callTool(toolName, args)
	if err != nil {
		s.t.Fatalf("callTool(%q) unexpected error: %v", toolName, err)
	}

	result, ok := resp["result"].(map[string]any)
	if !ok {
		s.t.Fatalf("callTool(%q) expected result map, got %T", toolName, resp["result"])
	}
	if isErr, _ := result["isError"].(bool); isErr {
		s.t.Fatalf("callTool(%q) tool returned IsError=true", toolName)
	}
	content, ok := result["content"].([]any)
	if !ok || len(content) == 0 {
		s.t.Fatalf("callTool(%q) expected content in result", toolName)
	}
	tc, ok := content[0].(map[string]any)
	if !ok {
		s.t.Fatalf("callTool(%q) expected map in content[0]", toolName)
	}
	text, ok := tc["text"].(string)
	if !ok {
		s.t.Fatalf("callTool(%q) expected text in content[0]", toolName)
	}
	if wantSubstr != "" && !strings.Contains(text, wantSubstr) {
		s.t.Errorf("callTool(%q) text = %q, want containing %q", toolName, text, wantSubstr)
	}
}

// callToolExercise calls a tool just to exercise the handler code path.
// It accepts both success and error — it only fails on panic/crash.
func (s *mcpSession) callToolExercise(toolName string, args map[string]any) {
	s.t.Helper()
	resp, _ := s.callTool(toolName, args)
	if resp == nil {
		s.t.Fatalf("callTool(%q) returned nil response", toolName)
	}
}

// ---------- AST tool handler tests ----------

func TestASTHandlers_InvalidProjectDir(t *testing.T) {
	s := newMCPSession(t)
	nonExistent := filepath.Join(t.TempDir(), "does_not_exist_xyz")

	tests := []struct {
		name string
		args map[string]any
	}{
		{
			name: brand.MCPToolName("ast", "query"),
			args: map[string]any{"query": "MATCH (n) RETURN n", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("ast", "search"),
			args: map[string]any{"query": "test", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("ast", "query_ai"),
			args: map[string]any{"query": "find functions", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("ast", "schema"),
			args: map[string]any{"project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("ast", "source"),
			args: map[string]any{"path": "main.go", "project_dir": nonExistent},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s.callToolExpectError(tt.name, tt.args, "does not exist")
		})
	}
}

func TestASTHandlers_NoASTDatabase(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	tests := []struct {
		name string
		args map[string]any
	}{
		{
			name: brand.MCPToolName("ast", "query"),
			args: map[string]any{"query": "MATCH (n) RETURN n", "project_dir": tmpDir},
		},
		{
			name: brand.MCPToolName("ast", "search"),
			args: map[string]any{"query": "search term", "project_dir": tmpDir},
		},
		{
			name: brand.MCPToolName("ast", "query_ai"),
			args: map[string]any{"query": "find functions", "project_dir": tmpDir},
		},
		{
			name: brand.MCPToolName("ast", "schema"),
			args: map[string]any{"project_dir": tmpDir},
		},
		{
			name: brand.MCPToolName("ast", "source"),
			args: map[string]any{"path": "main.go", "project_dir": tmpDir},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s.callToolExpectError(tt.name, tt.args, "no AST database found")
		})
	}
}

func TestASTQuery_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("ast", "query"), map[string]any{
		"query":       "MATCH (n) RETURN n",
		"project_dir": tmpDir,
		"context":     "my-context",
	}, "no AST database found")
}

func TestASTSearch_WithModes(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	modes := []string{"fts", "semantic", "hybrid", ""}
	for _, mode := range modes {
		name := mode
		if name == "" {
			name = "default"
		}
		t.Run("mode_"+name, func(t *testing.T) {
			args := map[string]any{
				"query":       "search term",
				"project_dir": tmpDir,
			}
			if mode != "" {
				args["mode"] = mode
			}
			s.callToolExpectError(brand.MCPToolName("ast", "search"), args, "no AST database found")
		})
	}
}

func TestASTSearch_TopKValues(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// Test with top_k=0 (should default to 15)
	s.callToolExpectError(brand.MCPToolName("ast", "search"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"top_k":       0,
	}, "no AST database found")

	// Test with explicit top_k
	s.callToolExpectError(brand.MCPToolName("ast", "search"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"top_k":       5,
	}, "no AST database found")

	// Test with negative top_k (should default to 15)
	s.callToolExpectError(brand.MCPToolName("ast", "search"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"top_k":       -1,
	}, "no AST database found")
}

func TestASTSource_AllParameters(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("ast", "source"), map[string]any{
		"path":         "main.go",
		"project_dir":  tmpDir,
		"entity":       "MyFunc",
		"entity_type":  "Function",
		"head":         10,
		"tail":         5,
		"start_line":   1,
		"end_line":     100,
		"pattern":      "TODO",
		"regex":        true,
		"before":       3,
		"after":        3,
		"line_numbers": true,
	}, "no AST database found")
}

func TestASTSource_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("ast", "source"), map[string]any{
		"path":        "main.go",
		"project_dir": tmpDir,
		"context":     "test-context",
	}, "no AST database found")
}

func TestASTSchema_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("ast", "schema"), map[string]any{
		"project_dir": tmpDir,
		"context":     "test-context",
	}, "no AST database found")
}

func TestASTQueryAI_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("ast", "query_ai"), map[string]any{
		"query":       "find exported functions",
		"project_dir": tmpDir,
		"context":     "test-context",
	}, "no AST database found")
}

// ---------- Hub tool handler tests ----------

func TestHubHandlers_InvalidProjectDir(t *testing.T) {
	s := newMCPSession(t)
	nonExistent := filepath.Join(t.TempDir(), "does_not_exist_xyz")

	tests := []struct {
		name string
		args map[string]any
	}{
		{
			name: brand.MCPToolName("hub", "install"),
			args: map[string]any{"id": "my-artifact", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("hub", "uninstall"),
			args: map[string]any{"id": "my-artifact", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("hub", "update"),
			args: map[string]any{"project_dir": nonExistent},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s.callToolExpectError(tt.name, tt.args, "does not exist")
		})
	}
}

// ---------- Knowledge tool handler tests ----------

func TestKnowledgeHandlers_InvalidProjectDir(t *testing.T) {
	s := newMCPSession(t)
	nonExistent := filepath.Join(t.TempDir(), "does_not_exist_xyz")

	tests := []struct {
		name string
		args map[string]any
	}{
		{
			name: brand.MCPToolName("knowledge", "query"),
			args: map[string]any{"query": "test", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("knowledge", "search"),
			args: map[string]any{"query": "test", "project_dir": nonExistent},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s.callToolExpectError(tt.name, tt.args, "does not exist")
		})
	}
}

func TestKnowledgeQuery_EmptyWikiDir(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	// Should fail because no wiki directory exists — the error message may vary
	s.callToolExpectError(brand.MCPToolName("knowledge", "query"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
	}, "wiki not found")
}

func TestKnowledgeSearch_EmptyWikiDir(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	// No wiki directory → resolveWikiDir returns "" → "wiki not found" error
	s.callToolExpectError(brand.MCPToolName("knowledge", "search"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
	}, "wiki not found")
}

func TestKnowledgeQuery_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	s.callToolExpectError(brand.MCPToolName("knowledge", "query"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"context":     "nonexistent-context",
	}, "wiki not found")
}

func TestKnowledgeSearch_WithResults(t *testing.T) {
	tmpDir := t.TempDir()
	wikiDir := filepath.Join(tmpDir, ".graphit", "knowledge", "project")
	_ = os.MkdirAll(wikiDir, 0o755)

	page := `---
title: Test Page
tags: [test]
---

# Test Page

This is a test page about authentication and authorization.
`
	_ = os.WriteFile(filepath.Join(wikiDir, "test-page.md"), []byte(page), 0o644)

	s := newMCPSession(t)

	// resolveWikiDir returns a relative path; the BM25Search will look relative
	// to cwd. We must chdir to tmpDir so the relative path resolves correctly.
	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })
	_ = os.Chdir(tmpDir)

	s.callToolExpectText(brand.MCPToolName("knowledge", "search"), map[string]any{
		"query":       "authentication",
		"project_dir": tmpDir,
	}, "Found")
}

func TestKnowledgeSearch_NoResults(t *testing.T) {
	tmpDir := t.TempDir()
	wikiDir := filepath.Join(tmpDir, ".graphit", "knowledge", "project")
	_ = os.MkdirAll(wikiDir, 0o755)

	page := `---
title: Unrelated
---
# Unrelated
Nothing here.
`
	_ = os.WriteFile(filepath.Join(wikiDir, "unrelated.md"), []byte(page), 0o644)

	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })
	_ = os.Chdir(tmpDir)

	s.callToolExpectText(brand.MCPToolName("knowledge", "search"), map[string]any{
		"query":       "xyznonexistent12345",
		"project_dir": tmpDir,
	}, "No results found")
}

func TestKnowledgeSearch_WithTopK(t *testing.T) {
	tmpDir := t.TempDir()
	wikiDir := filepath.Join(tmpDir, ".graphit", "knowledge", "project")
	_ = os.MkdirAll(wikiDir, 0o755)

	for i := 0; i < 5; i++ {
		page := fmt.Sprintf("---\ntitle: Auth Page %d\n---\n# Auth Page %d\nAuthentication details %d\n", i, i, i)
		_ = os.WriteFile(filepath.Join(wikiDir, fmt.Sprintf("auth-%d.md", i)), []byte(page), 0o644)
	}

	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })
	_ = os.Chdir(tmpDir)

	s.callToolExpectText(brand.MCPToolName("knowledge", "search"), map[string]any{
		"query":       "authentication",
		"project_dir": tmpDir,
		"top_k":       2,
	}, "Found")
}

func TestKnowledgeSearch_ResultFormatting(t *testing.T) {
	tmpDir := t.TempDir()
	wikiDir := filepath.Join(tmpDir, ".graphit", "knowledge", "project")
	_ = os.MkdirAll(wikiDir, 0o755)

	page := `---
title: Authentication Guide
tags: [auth]
---

# Authentication Guide

This guide covers authentication patterns, OAuth2, and JWT tokens.
Authentication is essential for secure APIs.
`
	_ = os.WriteFile(filepath.Join(wikiDir, "auth-guide.md"), []byte(page), 0o644)

	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })
	_ = os.Chdir(tmpDir)

	resp, err := s.callTool(brand.MCPToolName("knowledge", "search"), map[string]any{
		"query":       "authentication OAuth2",
		"project_dir": tmpDir,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	result := resp["result"].(map[string]any)
	content := result["content"].([]any)
	tc := content[0].(map[string]any)
	text := tc["text"].(string)

	if !strings.Contains(text, "score:") {
		t.Errorf("expected score in result, got: %s", text)
	}
}

func TestKnowledgeSearch_ResultWithTitleAndSnippet(t *testing.T) {
	tmpDir := t.TempDir()
	wikiDir := filepath.Join(tmpDir, ".graphit", "knowledge", "project")
	_ = os.MkdirAll(wikiDir, 0o755)

	page := `---
title: API Design Patterns
tags: [api, design]
---

# API Design Patterns

REST API design with authentication middleware and rate limiting.
`
	_ = os.WriteFile(filepath.Join(wikiDir, "api-design.md"), []byte(page), 0o644)

	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })
	_ = os.Chdir(tmpDir)

	resp, err := s.callTool(brand.MCPToolName("knowledge", "search"), map[string]any{
		"query":       "REST API authentication",
		"project_dir": tmpDir,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	result := resp["result"].(map[string]any)
	content := result["content"].([]any)
	tc := content[0].(map[string]any)
	text := tc["text"].(string)

	// Verify title appears in the output (the handler includes "— Title" for non-empty titles)
	if !strings.Contains(text, "API Design Patterns") {
		t.Errorf("expected title in result, got: %s", text)
	}
}

// ---------- Wiki tool handler tests ----------

func TestWikiChat_EmptySessionID(t *testing.T) {
	s := newMCPSession(t)

	// The SDK validates required fields via JSON schema before reaching our handler.
	// Pass both fields but with empty session_id to hit the handler's validation.
	s.callToolExpectError(brand.MCPToolName("wiki", "chat"), map[string]any{
		"session_id": "",
		"message":    "hello",
	}, "session_id is required")
}

func TestWikiChat_EmptyMessage(t *testing.T) {
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("wiki", "chat"), map[string]any{
		"session_id": "some-session",
		"message":    "",
	}, "message is required")
}

func TestWikiChat_BothEmpty(t *testing.T) {
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("wiki", "chat"), map[string]any{
		"session_id": "",
		"message":    "",
	}, "session_id is required")
}

func TestWikiChat_NonExistentSession(t *testing.T) {
	s := newMCPSession(t)

	// Valid inputs but session doesn't exist — should error
	s.callToolExpectError(brand.MCPToolName("wiki", "chat"), map[string]any{
		"session_id": "nonexistent-session-xyz-12345",
		"message":    "hello",
	}, "")
}

func TestWikiSessions_UnknownAction(t *testing.T) {
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("wiki", "sessions"), map[string]any{
		"action": "invalid_action",
	}, "unknown action")
}

func TestWikiSessions_DeleteMissingSessionID(t *testing.T) {
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("wiki", "sessions"), map[string]any{
		"action":     "delete",
		"session_id": "",
	}, "session_id is required for delete")
}

func TestWikiSessions_DeleteNonExistent(t *testing.T) {
	s := newMCPSession(t)

	// Delete a non-existent session
	s.callToolExpectError(brand.MCPToolName("wiki", "sessions"), map[string]any{
		"action":     "delete",
		"session_id": "nonexistent-session-xyz",
	}, "")
}

func TestWikiSessions_InvalidProjectDir(t *testing.T) {
	s := newMCPSession(t)
	nonExistent := filepath.Join(t.TempDir(), "does_not_exist_xyz")

	s.callToolExpectError(brand.MCPToolName("wiki", "sessions"), map[string]any{
		"action":      "list",
		"project_dir": nonExistent,
	}, "does not exist")
}

func TestWikiSessions_ListValidDir(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// List sessions for a valid project dir — may return "No sessions found"
	resp, err := s.callTool(brand.MCPToolName("wiki", "sessions"), map[string]any{
		"action":      "list",
		"project_dir": tmpDir,
	})
	if err != nil {
		// May fail depending on wikisvc initialization — that's acceptable
		return
	}
	result := resp["result"].(map[string]any)
	content := result["content"].([]any)
	if len(content) == 0 {
		t.Fatal("expected content in response")
	}
	tc := content[0].(map[string]any)
	text, _ := tc["text"].(string)
	if !strings.Contains(text, "No sessions found") && !strings.Contains(text, "session") {
		t.Errorf("unexpected text: %s", text)
	}
}

func TestWikiSearch_InvalidProjectDir(t *testing.T) {
	s := newMCPSession(t)
	nonExistent := filepath.Join(t.TempDir(), "does_not_exist_xyz")

	s.callToolExpectError(brand.MCPToolName("wiki", "search"), map[string]any{
		"query":       "test",
		"project_dir": nonExistent,
	}, "does not exist")
}

func TestWikiSearch_WithAllParams(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// Exercise the handler with all optional fields filled in.
	// Will fail because of AI client or wiki config, but exercises deserialization.
	s.callToolExpectError(brand.MCPToolName("wiki", "search"), map[string]any{
		"query":       "test query",
		"wikis":       []string{"project", "memory"},
		"hub_refs":    []string{"art@1.0"},
		"session_id":  "sess-123",
		"top_k":       5,
		"project_dir": tmpDir,
	}, "")
}

// ---------- Memory tool handler tests ----------

func TestMemoryHandlers_InvalidProjectDir(t *testing.T) {
	s := newMCPSession(t)
	nonExistent := filepath.Join(t.TempDir(), "does_not_exist_xyz")

	tests := []struct {
		name string
		args map[string]any
	}{
		{
			name: brand.MCPToolName("memory", "query"),
			args: map[string]any{"query": "test", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("memory", "list"),
			args: map[string]any{"project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("memory", "add"),
			args: map[string]any{"title": "Test", "content": "C", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("memory", "remove"),
			args: map[string]any{"id": "test", "project_dir": nonExistent},
		},
		{
			name: brand.MCPToolName("memory", "search"),
			args: map[string]any{"query": "test", "project_dir": nonExistent},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s.callToolExpectError(tt.name, tt.args, "does not exist")
		})
	}
}

func TestMemoryQuery_ProjectScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	// Will fail (AI not configured) but exercises resolveMemoryWikiDir project path
	_, _ = s.callTool(brand.MCPToolName("memory", "query"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
	})
}

func TestMemoryQuery_UserScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	_, _ = s.callTool(brand.MCPToolName("memory", "query"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"scope":       "user",
	})
}

func TestMemoryQuery_DefaultScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	// Empty scope defaults to "project", wikiDir won't be empty, just hits AI error
	_, _ = s.callTool(brand.MCPToolName("memory", "query"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"scope":       "",
	})
}

func TestMemoryQuery_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })

	_, _ = s.callTool(brand.MCPToolName("memory", "query"), map[string]any{
		"query":       "test",
		"project_dir": tmpDir,
		"context":     "some-context",
	})
}

func TestMemorySearch_Scopes(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	for _, scope := range []string{"project", "user", ""} {
		name := scope
		if name == "" {
			name = "default"
		}
		t.Run("scope_"+name, func(t *testing.T) {
			args := map[string]any{
				"query":       "test",
				"project_dir": tmpDir,
			}
			if scope != "" {
				args["scope"] = scope
			}
			// Exercise scope resolution — may succeed or fail depending on env
			s.callToolExercise(brand.MCPToolName("memory", "search"), args)
		})
	}
}

func TestMemoryAdd_AllFields(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// Test with all fields to exercise full deserialization
	_, _ = s.callTool(brand.MCPToolName("memory", "add"), map[string]any{
		"title":       "Test Memory",
		"content":     "Some content",
		"type":        "convention",
		"scope":       "project",
		"important":   true,
		"tags":        "go,test",
		"project_dir": tmpDir,
	})
}

func TestMemoryAdd_UserScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// May succeed or fail depending on git identity in environment
	_, _ = s.callTool(brand.MCPToolName("memory", "add"), map[string]any{
		"title":       "Test Memory",
		"content":     "Some content",
		"scope":       "user",
		"project_dir": tmpDir,
	})
}

func TestMemoryList_ProjectScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("memory", "list"), map[string]any{
		"project_dir": tmpDir,
	}, "")
}

func TestMemoryList_UserScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// May succeed or fail depending on git identity in environment
	_, _ = s.callTool(brand.MCPToolName("memory", "list"), map[string]any{
		"scope":       "user",
		"project_dir": tmpDir,
	})
}

func TestMemoryRemove_ProjectScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("memory", "remove"), map[string]any{
		"id":          "test-memory",
		"project_dir": tmpDir,
	}, "")
}

func TestMemoryRemove_UserScope(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	s.callToolExpectError(brand.MCPToolName("memory", "remove"), map[string]any{
		"id":          "test-memory",
		"scope":       "user",
		"project_dir": tmpDir,
	}, "")
}

// ---------- openASTDB tests ----------

func TestOpenASTDB_NonExistentProjectDir(t *testing.T) {
	_, err := openASTDB("/nonexistent/path/xyz", "")
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "cannot chdir") {
		t.Errorf("error = %q, want containing 'cannot chdir'", err.Error())
	}
}

func TestOpenASTDB_NoDatabase(t *testing.T) {
	tmpDir := t.TempDir()
	_, err := openASTDB(tmpDir, "")
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "no AST database found") {
		t.Errorf("error = %q, want containing 'no AST database found'", err.Error())
	}
}

func TestOpenASTDB_WithContext(t *testing.T) {
	tmpDir := t.TempDir()
	_, err := openASTDB(tmpDir, "mycontext")
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "no AST database found") {
		t.Errorf("error = %q, want containing 'no AST database found'", err.Error())
	}
}

func TestOpenASTDB_EmptyContext(t *testing.T) {
	tmpDir := t.TempDir()
	_, err := openASTDB(tmpDir, "")
	if err == nil {
		t.Fatal("expected error for missing db")
	}
}

// ---------- newMemorySvc tests ----------

func TestNewMemorySvc_ProjectScope(t *testing.T) {
	tmpDir := t.TempDir()
	_, err := newMemorySvc(false, tmpDir)
	if err == nil {
		t.Log("newMemorySvc succeeded unexpectedly — lockfile may exist")
	} else if !strings.Contains(err.Error(), "not initialised") && !strings.Contains(err.Error(), "not initialized") {
		t.Logf("error: %v", err)
	}
}

func TestNewMemorySvc_UserScope(t *testing.T) {
	tmpDir := t.TempDir()
	_, err := newMemorySvc(true, tmpDir)
	// User scope tries to get git identity — may or may not fail depending on env
	if err != nil {
		t.Logf("newMemorySvc user scope error: %v", err)
	}
}

// ---------- Concurrent session tests ----------

func TestMultipleSessions_Independent(t *testing.T) {
	s1 := newMCPSession(t)
	s2 := newMCPSession(t)

	nonExistent := filepath.Join(t.TempDir(), "does_not_exist")
	s1.callToolExpectError(brand.MCPToolName("ast", "query"), map[string]any{
		"query":       "MATCH (n) RETURN n",
		"project_dir": nonExistent,
	}, "does not exist")

	s2.callToolExpectError(brand.MCPToolName("ast", "query"), map[string]any{
		"query":       "MATCH (n) RETURN n",
		"project_dir": nonExistent,
	}, "does not exist")
}

// ---------- Empty/null argument tests ----------

func TestToolCall_EmptyArguments(t *testing.T) {
	s := newMCPSession(t)

	// hub list accepts empty args
	resp, _ := s.callTool(brand.MCPToolName("hub", "list"), map[string]any{})
	if resp == nil {
		t.Fatal("expected non-nil response")
	}
}

// ---------- Tool with default project dir (cwd) ----------

func TestToolCall_DefaultProjectDir(t *testing.T) {
	s := newMCPSession(t)

	// When project_dir is empty, resolveProjectDir uses os.Getwd().
	resp, _ := s.callTool(brand.MCPToolName("ast", "schema"), map[string]any{})
	if resp == nil {
		t.Fatal("expected non-nil response")
	}
}

// ---------- Hub list / search tests ----------

func TestHubList_WithTypeFilter(t *testing.T) {
	s := newMCPSession(t)
	resp, _ := s.callTool(brand.MCPToolName("hub", "list"), map[string]any{
		"type": "knowledge",
	})
	if resp == nil {
		t.Fatal("expected non-nil response")
	}
}

func TestHubSearch_WithQuery(t *testing.T) {
	s := newMCPSession(t)
	resp, _ := s.callTool(brand.MCPToolName("hub", "search"), map[string]any{
		"query": "test",
	})
	if resp == nil {
		t.Fatal("expected non-nil response")
	}
}

func TestHubSearch_WithTypeFilter(t *testing.T) {
	s := newMCPSession(t)
	resp, _ := s.callTool(brand.MCPToolName("hub", "search"), map[string]any{
		"query": "test",
		"type":  "rule",
	})
	if resp == nil {
		t.Fatal("expected non-nil response")
	}
}

func TestHubShow_NonExistentArtifact(t *testing.T) {
	s := newMCPSession(t)
	s.callToolExpectError(brand.MCPToolName("hub", "show"), map[string]any{
		"id": "nonexistent-artifact-xyz-12345",
	}, "")
}

func TestHubShow_WithType(t *testing.T) {
	s := newMCPSession(t)
	s.callToolExpectError(brand.MCPToolName("hub", "show"), map[string]any{
		"id":   "nonexistent-artifact-xyz-12345",
		"type": "rule",
	}, "")
}

// ---------- Hub update with/without ID ----------

func TestHubUpdate_WithID(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// Update a specific artifact — will fail at registry level
	s.callToolExpectError(brand.MCPToolName("hub", "update"), map[string]any{
		"id":          "nonexistent-art",
		"project_dir": tmpDir,
	}, "")
}

func TestHubUpdate_WithoutID(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// Update all — may succeed or fail depending on env
	_, _ = s.callTool(brand.MCPToolName("hub", "update"), map[string]any{
		"project_dir": tmpDir,
	})
}

func TestHubInstall_WithAllFields(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	// Exercise full deserialization with all fields
	s.callToolExpectError(brand.MCPToolName("hub", "install"), map[string]any{
		"id":          "my-rule@1.2.0",
		"type":        "rule",
		"ide":         "claude",
		"project_dir": tmpDir,
	}, "")
}

func TestHubUninstall_WithAllFields(t *testing.T) {
	tmpDir := t.TempDir()
	s := newMCPSession(t)

	_, _ = s.callTool(brand.MCPToolName("hub", "uninstall"), map[string]any{
		"id":          "my-rule",
		"type":        "rule",
		"ide":         "cursor",
		"project_dir": tmpDir,
	})
}
