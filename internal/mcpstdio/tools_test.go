package mcpstdio

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestTextResult(t *testing.T) {
	result, session, err := textResult("hello world")
	if err != nil {
		t.Fatalf("textResult() error: %v", err)
	}
	if session != nil {
		t.Errorf("session = %v; want nil", session)
	}
	if result == nil {
		t.Fatal("result is nil")
	}
	if len(result.Content) != 1 {
		t.Fatalf("content length = %d; want 1", len(result.Content))
	}
	tc, ok := result.Content[0].(*mcp.TextContent)
	if !ok {
		t.Fatalf("content[0] type = %T; want *mcp.TextContent", result.Content[0])
	}
	want := "hello world" + ide.SysReminder
	if tc.Text != want {
		t.Errorf("text = %q; want %q", tc.Text, want)
	}
}

func TestTextResult_Empty(t *testing.T) {
	result, _, err := textResult("")
	if err != nil {
		t.Fatalf("textResult() error: %v", err)
	}
	if result == nil || len(result.Content) != 1 {
		t.Fatal("expected 1 content item")
	}
	tc := result.Content[0].(*mcp.TextContent)
	want := ide.SysReminder
	if tc.Text != want {
		t.Errorf("text = %q; want %q", tc.Text, want)
	}
}

func TestErrResult(t *testing.T) {
	testErr := fmt.Errorf("something failed")
	result, session, err := errResult(testErr)
	if result != nil {
		t.Errorf("result = %v; want nil", result)
	}
	if session != nil {
		t.Errorf("session = %v; want nil", session)
	}
	if !errors.Is(err, testErr) {
		t.Errorf("err = %v; want %v", err, testErr)
	}
}

func TestJsonResult(t *testing.T) {
	type sample struct {
		Name  string `json:"name"`
		Count int    `json:"count"`
	}
	input := sample{Name: "test", Count: 42}

	result, session, err := jsonResult(input)
	if err != nil {
		t.Fatalf("jsonResult() error: %v", err)
	}
	if session != nil {
		t.Errorf("session = %v; want nil", session)
	}
	if result == nil || len(result.Content) < 1 {
		t.Fatal("expected at least 1 content item")
	}
	tc := result.Content[0].(*mcp.TextContent)

	var parsed sample
	if err := json.Unmarshal([]byte(tc.Text), &parsed); err != nil {
		t.Fatalf("failed to parse JSON output: %v", err)
	}
	if parsed.Name != "test" || parsed.Count != 42 {
		t.Errorf("parsed = %+v; want {Name:test Count:42}", parsed)
	}
	// Verify reminder is present if SysReminder is not empty
	if ide.SysReminder != "" {
		if len(result.Content) < 2 {
			t.Fatal("expected reminder content block")
		}
		reminder := result.Content[1].(*mcp.TextContent)
		if !strings.Contains(reminder.Text, "_SYS_REMINDER") {
			t.Error("expected _SYS_REMINDER in second content block")
		}
	} else {
		if len(result.Content) != 1 {
			t.Errorf("expected 1 content block, got %d", len(result.Content))
		}
	}
}

func TestJsonResult_Indented(t *testing.T) {
	result, _, err := jsonResult(map[string]string{"key": "value"})
	if err != nil {
		t.Fatal(err)
	}
	tc := result.Content[0].(*mcp.TextContent)
	// MarshalIndent should produce newlines and spaces
	if tc.Text == `{"key":"value"}` {
		t.Error("expected indented JSON output")
	}
}

func TestJsonResult_Unmarshalable(t *testing.T) {
	// Channels cannot be marshaled to JSON
	_, _, err := jsonResult(make(chan int))
	if err == nil {
		t.Error("expected error for unmarshalable type")
	}
}

func TestSafeTool_PanicRecovery(t *testing.T) {
	type dummyInput struct{}
	panicker := func(ctx context.Context, req *mcp.CallToolRequest, input dummyInput) (*mcp.CallToolResult, any, error) {
		panic("test panic")
	}

	wrapped := safeTool(panicker)
	result, session, err := wrapped(context.Background(), &mcp.CallToolRequest{}, dummyInput{})
	if err == nil {
		t.Fatal("expected error from panic recovery")
	}
	if result != nil {
		t.Errorf("result = %v; want nil after panic", result)
	}
	if session != nil {
		t.Errorf("session = %v; want nil after panic", session)
	}
	expected := "internal error (panic): test panic"
	if err.Error() != expected {
		t.Errorf("error = %q; want %q", err.Error(), expected)
	}
}

func TestSafeTool_NormalExecution(t *testing.T) {
	type dummyInput struct{}
	normal := func(ctx context.Context, req *mcp.CallToolRequest, input dummyInput) (*mcp.CallToolResult, any, error) {
		return textResult("ok")
	}

	wrapped := safeTool(normal)
	result, _, err := wrapped(context.Background(), &mcp.CallToolRequest{}, dummyInput{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result == nil {
		t.Fatal("expected non-nil result")
	}
	tc := result.Content[0].(*mcp.TextContent)
	want := "ok" + ide.SysReminder
	if tc.Text != want {
		t.Errorf("text = %q; want %q", tc.Text, want)
	}
}

func TestSanitizeContextName(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		want    string
		wantErr bool
	}{
		{name: "simple name", input: "mycontext", want: "mycontext"},
		{name: "empty name", input: "", wantErr: true},
		{name: "dot only", input: ".", wantErr: true},
		{name: "dotdot", input: "..", wantErr: true},
		{name: "path traversal", input: "../../etc/passwd", want: "passwd"},
		{name: "absolute path", input: "/usr/local/bin/ctx", want: "ctx"},
		{name: "with directory", input: "foo/bar", want: "bar"},
		{name: "name with dots", input: "my.context", want: "my.context"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := sanitizeContextName(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Error("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Errorf("sanitizeContextName(%q) = %q; want %q", tt.input, got, tt.want)
			}
		})
	}
}

func TestResolveProjectDir(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		wantErr bool
	}{
		{name: "empty string", input: "", wantErr: true},
		{name: "nonexistent directory", input: "/tmp/definitely-does-not-exist-graphit-test-xxyz", wantErr: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := resolveProjectDir(tt.input)
			if tt.wantErr && err == nil {
				t.Error("expected error, got nil")
			}
			if !tt.wantErr && err != nil {
				t.Errorf("unexpected error: %v", err)
			}
		})
	}

	// Test with a real directory
	t.Run("valid directory", func(t *testing.T) {
		tmp := t.TempDir()
		result, err := resolveProjectDir(tmp)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if result == "" {
			t.Error("expected non-empty result")
		}
	})
}

func TestWithProjectDir(t *testing.T) {
	origDir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}

	tmp := t.TempDir()

	// Test successful chdir and restoration
	var insideDir string
	err = withProjectDir(tmp, func() error {
		insideDir, _ = os.Getwd()
		return nil
	})
	if err != nil {
		t.Fatalf("withProjectDir error: %v", err)
	}

	// Verify we were in the temp dir during execution
	if insideDir != tmp {
		// On some OSes, the path may be resolved differently (symlinks, etc.)
		// Just check it's not the original
		if insideDir == origDir {
			t.Error("expected to be in temp dir during execution")
		}
	}

	// Verify we're back to original
	currentDir, _ := os.Getwd()
	if currentDir != origDir {
		t.Errorf("cwd not restored: got %q, want %q", currentDir, origDir)
	}
}

func TestWithProjectDir_InvalidDir(t *testing.T) {
	err := withProjectDir("/nonexistent-dir-graphit-test", func() error {
		return nil
	})
	if err == nil {
		t.Error("expected error for nonexistent directory")
	}
}

func TestWithProjectDir_FuncError(t *testing.T) {
	tmp := t.TempDir()
	expectedErr := fmt.Errorf("function error")
	err := withProjectDir(tmp, func() error {
		return expectedErr
	})
	if !errors.Is(err, expectedErr) {
		t.Errorf("error = %v; want %v", err, expectedErr)
	}
}

func TestSplitLastNLocal(t *testing.T) {
	tests := []struct {
		name  string
		input string
		n     int
		want  int // expected number of lines
	}{
		{name: "fewer lines than n", input: "a\nb\n", n: 5, want: 2},
		{name: "more lines than n", input: "a\nb\nc\nd\ne\n", n: 2, want: 2},
		{name: "exact n lines", input: "a\nb\nc\n", n: 3, want: 3},
		{name: "empty string", input: "", n: 5, want: 0},
		{name: "single line", input: "hello\n", n: 5, want: 1},
		{name: "no trailing newline", input: "a\nb\nc", n: 5, want: 3},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := splitLastNLocal(tt.input, tt.n)
			if len(result) != tt.want {
				t.Errorf("splitLastNLocal(%q, %d) = %d lines; want %d (got: %v)",
					tt.input, tt.n, len(result), tt.want, result)
			}
		})
	}
}

func TestScopeFromString(t *testing.T) {
	tests := []struct {
		input string
		want  bool
	}{
		{"user", true},
		{"project", false},
		{"", false},
		{"User", false},
	}
	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := scopeFromString(tt.input)
			if got != tt.want {
				t.Errorf("scopeFromString(%q) = %v; want %v", tt.input, got, tt.want)
			}
		})
	}
}

func TestNopWriteCloser(t *testing.T) {
	var buf bytes.Buffer
	wc := nopWriteCloser{&buf}

	_, err := wc.Write([]byte("hello"))
	if err != nil {
		t.Fatalf("Write error: %v", err)
	}
	if buf.String() != "hello" {
		t.Errorf("buf = %q; want %q", buf.String(), "hello")
	}

	if err := wc.Close(); err != nil {
		t.Errorf("Close() error: %v", err)
	}
}

// ---------------------------------------------------------------------------
// hub type-path MCP tool
// ---------------------------------------------------------------------------

func TestHubTypePathTool(t *testing.T) {
	ctx := context.Background()

	server := NewServer()
	clientT, serverT := mcp.NewInMemoryTransports()

	ss, err := server.Connect(ctx, serverT, nil)
	if err != nil {
		t.Fatalf("server connect: %v", err)
	}
	defer func() { _ = ss.Close() }()

	client := mcp.NewClient(&mcp.Implementation{Name: "test", Version: "0"}, nil)
	cs, err := client.Connect(ctx, clientT, nil)
	if err != nil {
		t.Fatalf("client connect: %v", err)
	}
	defer func() { _ = cs.Close() }()

	toolName := brand.MCPToolName("hub", "type-path")

	// 1. The tool must be registered.
	list, err := cs.ListTools(ctx, nil)
	if err != nil {
		t.Fatalf("list tools: %v", err)
	}
	found := false
	for _, tl := range list.Tools {
		if tl.Name == toolName {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected tool %q to be registered", toolName)
	}

	// 2. Calling it must return a path for the requested skill/name.
	dir := t.TempDir()
	res, err := cs.CallTool(ctx, &mcp.CallToolParams{
		Name: toolName,
		Arguments: map[string]any{
			"project_dir": dir,
			"type":        "skill",
			"name":        "my-error-patterns",
			"ide":         "claude",
		},
	})
	if err != nil {
		t.Fatalf("call tool: %v", err)
	}
	if res.IsError {
		t.Fatalf("tool returned error result: %+v", res.Content)
	}
	var out string
	for _, c := range res.Content {
		if tc, ok := c.(*mcp.TextContent); ok {
			out += tc.Text
		}
	}
	if !strings.Contains(out, "my-error-patterns") {
		t.Errorf("expected path to contain artifact name, got %q", out)
	}
}
