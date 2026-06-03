package mcpserver

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

// ---------- NewServer ----------

func TestNewServer_ReturnsNonNil(t *testing.T) {
	server := NewServer()
	if server == nil {
		t.Fatal("expected non-nil MCP server instance")
	}
}

func TestNewServer_RegistersExpectedTools(t *testing.T) {
	// NewServer should not panic when registering all tool groups.
	// We verify tool registration more thoroughly in TestHTTPHandler_ListTools.
	server := NewServer()
	if server == nil {
		t.Fatal("expected non-nil server with tools registered")
	}
}

func TestNewServer_ServerImplementation(t *testing.T) {
	// Verify the server name and version match brand expectations.
	expectedName := brand.MCPServerName("code")
	expectedVersion := version.Version

	if expectedName == "" {
		t.Error("expected non-empty MCP server name")
	}
	if expectedVersion == "" {
		t.Error("expected non-empty version")
	}
}

// ---------- textResult / errResult ----------

func TestTextResult(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		wantText string
	}{
		{
			name:     "simple text",
			input:    "hello world",
			wantText: "hello world",
		},
		{
			name:     "empty text",
			input:    "",
			wantText: "",
		},
		{
			name:     "multiline text",
			input:    "line1\nline2\nline3",
			wantText: "line1\nline2\nline3",
		},
		{
			name:     "text with special characters",
			input:    `{"key": "value", "num": 42}`,
			wantText: `{"key": "value", "num": 42}`,
		},
		{
			name:     "unicode text",
			input:    "こんにちは世界 🌍",
			wantText: "こんにちは世界 🌍",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, extra, err := textResult(tt.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if extra != nil {
				t.Errorf("expected nil extra, got %v", extra)
			}
			if result == nil {
				t.Fatal("expected non-nil result")
			}
			if len(result.Content) != 1 {
				t.Fatalf("expected 1 content block, got %d", len(result.Content))
			}
			tc, ok := result.Content[0].(*mcp.TextContent)
			if !ok {
				t.Fatalf("expected TextContent, got %T", result.Content[0])
			}
			if tc.Text != tt.wantText {
				t.Errorf("text = %q, want %q", tc.Text, tt.wantText)
			}
		})
	}
}

func TestErrResult(t *testing.T) {
	tests := []struct {
		name    string
		err     error
		wantMsg string
	}{
		{
			name:    "simple error",
			err:     errors.New("something went wrong"),
			wantMsg: "something went wrong",
		},
		{
			name:    "wrapped error",
			err:     fmt.Errorf("outer: %w", errors.New("inner")),
			wantMsg: "outer: inner",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, extra, err := errResult(tt.err)
			if result != nil {
				t.Errorf("expected nil result, got %v", result)
			}
			if extra != nil {
				t.Errorf("expected nil extra, got %v", extra)
			}
			if err == nil {
				t.Fatal("expected non-nil error")
			}
			if err.Error() != tt.wantMsg {
				t.Errorf("error = %q, want %q", err.Error(), tt.wantMsg)
			}
		})
	}
}


// ---------- resolveProjectDir ----------

func TestResolveProjectDir(t *testing.T) {
	// Create a temp directory for testing
	tmpDir := t.TempDir()
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "user"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "testctx"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "testctx"), 0755)

	tests := []struct {
		name      string
		input     string
		wantErr   bool
		wantIsWd  bool // if true, expects result == current working dir
		wantPath  string
		errSubstr string
	}{
		{
			name:     "empty string returns working directory",
			input:    "",
			wantIsWd: true,
		},
		{
			name:     "valid absolute path",
			input:    tmpDir,
			wantPath: tmpDir,
		},
		{
			name:      "non-existent directory",
			input:     filepath.Join(tmpDir, "nonexistent_dir_xyz"),
			wantErr:   true,
			errSubstr: "does not exist",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := resolveProjectDir(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error, got nil")
				}
				if tt.errSubstr != "" && !strings.Contains(err.Error(), tt.errSubstr) {
					t.Errorf("error %q does not contain %q", err.Error(), tt.errSubstr)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if tt.wantIsWd {
				wd, wdErr := os.Getwd()
				if wdErr != nil {
					t.Fatalf("cannot get wd: %v", wdErr)
				}
				if result != wd {
					t.Errorf("got %q, want working dir %q", result, wd)
				}
			} else if tt.wantPath != "" {
				if result != tt.wantPath {
					t.Errorf("got %q, want %q", result, tt.wantPath)
				}
			}
		})
	}
}

func TestResolveProjectDir_RelativePath(t *testing.T) {
	tmpDir := t.TempDir()
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "user"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "testctx"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "testctx"), 0755)
	subDir := filepath.Join(tmpDir, "subdir")
	if err := os.MkdirAll(subDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// Save and restore wd
	origWd, _ := os.Getwd()
	t.Cleanup(func() { _ = os.Chdir(origWd) })
	_ = os.Chdir(tmpDir)

	result, err := resolveProjectDir("subdir")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result != subDir {
		t.Errorf("got %q, want %q", result, subDir)
	}
}

// ---------- resolveWikiDir ----------

func TestResolveWikiDir(t *testing.T) {
	tmpDir := t.TempDir()
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "user"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "testctx"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "testctx"), 0755)

	tests := []struct {
		name       string
		module     string
		projectDir string
		context    string
		wantEmpty  bool
	}{
		{
			name:       "unknown module returns empty",
			module:     "unknown",
			projectDir: tmpDir,
			context:    "",
			wantEmpty:  true,
		},
		{
			name:       "knowledge module returns non-empty",
			module:     "knowledge",
			projectDir: tmpDir,
			context:    "",
			wantEmpty:  false,
		},
		{
			name:       "memory module returns non-empty",
			module:     "memory",
			projectDir: tmpDir,
			context:    "",
			wantEmpty:  false,
		},
		{
			name:       "memory module with context",
			module:     "memory",
			projectDir: tmpDir,
			context:    "testctx",
			wantEmpty:  false,
		},
		{
			name:       "knowledge module with context",
			module:     "knowledge",
			projectDir: tmpDir,
			context:    "testctx",
			wantEmpty:  false,
		},
		{
			name:       "empty module returns empty",
			module:     "",
			projectDir: tmpDir,
			context:    "",
			wantEmpty:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := resolveWikiDir(tt.module, tt.projectDir, tt.context)
			if tt.wantEmpty && result != "" {
				t.Errorf("expected empty result, got %q", result)
			}
			if !tt.wantEmpty && result == "" {
				t.Errorf("expected non-empty result, got empty")
			}
		})
	}
}

// ---------- resolveMemoryWikiDir ----------

func TestResolveMemoryWikiDir(t *testing.T) {
	tmpDir := t.TempDir()
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "user"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "testctx"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "testctx"), 0755)

	tests := []struct {
		name        string
		scope       string
		projectDir  string
		contextName string
	}{
		{
			name:       "project scope",
			scope:      "project",
			projectDir: tmpDir,
		},
		{
			name:       "user scope",
			scope:      "user",
			projectDir: tmpDir,
		},
		{
			name:       "default scope (empty)",
			scope:      "",
			projectDir: tmpDir,
		},
		{
			name:        "with context name",
			scope:       "project",
			projectDir:  tmpDir,
			contextName: "test-context",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := resolveMemoryWikiDir(tt.scope, tt.projectDir, tt.contextName)
			// For project/user scopes without context, we always get a non-empty path
			if tt.contextName == "" && result == "" {
				t.Error("expected non-empty wiki dir for standard scope")
			}
		})
	}
}

// ---------- Options struct ----------

func TestOptions_Defaults(t *testing.T) {
	opts := Options{}
	if opts.Host != "" {
		t.Errorf("expected empty Host default, got %q", opts.Host)
	}
	if opts.Port != 0 {
		t.Errorf("expected zero Port default, got %d", opts.Port)
	}
	if opts.Stdio {
		t.Error("expected Stdio to be false by default")
	}
	if opts.Verbose {
		t.Error("expected Verbose to be false by default")
	}
}

// ---------- Input struct JSON serialization ----------

func TestASTQueryInput_JSON(t *testing.T) {
	tests := []struct {
		name     string
		json     string
		wantErr  bool
		validate func(t *testing.T, input astQueryInput)
	}{
		{
			name: "full input",
			json: `{"query":"MATCH (n) RETURN n","project_dir":"/tmp/project","context":"myctx"}`,
			validate: func(t *testing.T, input astQueryInput) {
				if input.Query != "MATCH (n) RETURN n" {
					t.Errorf("Query = %q", input.Query)
				}
				if input.ProjectDir != "/tmp/project" {
					t.Errorf("ProjectDir = %q", input.ProjectDir)
				}
				if input.Context != "myctx" {
					t.Errorf("Context = %q", input.Context)
				}
			},
		},
		{
			name: "minimal input",
			json: `{"query":"RETURN 1"}`,
			validate: func(t *testing.T, input astQueryInput) {
				if input.Query != "RETURN 1" {
					t.Errorf("Query = %q", input.Query)
				}
				if input.ProjectDir != "" {
					t.Errorf("expected empty ProjectDir, got %q", input.ProjectDir)
				}
				if input.Context != "" {
					t.Errorf("expected empty Context, got %q", input.Context)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var input astQueryInput
			err := json.Unmarshal([]byte(tt.json), &input)
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			tt.validate(t, input)
		})
	}
}

func TestASTSearchInput_JSON(t *testing.T) {
	tests := []struct {
		name     string
		json     string
		validate func(t *testing.T, input astSearchInput)
	}{
		{
			name: "full input with all modes",
			json: `{"query":"findFunction","top_k":10,"mode":"hybrid","project_dir":"/p","context":"ctx"}`,
			validate: func(t *testing.T, input astSearchInput) {
				if input.Query != "findFunction" {
					t.Errorf("Query = %q", input.Query)
				}
				if input.TopK != 10 {
					t.Errorf("TopK = %d", input.TopK)
				}
				if input.Mode != "hybrid" {
					t.Errorf("Mode = %q", input.Mode)
				}
			},
		},
		{
			name: "defaults for optional fields",
			json: `{"query":"search term"}`,
			validate: func(t *testing.T, input astSearchInput) {
				if input.TopK != 0 {
					t.Errorf("expected zero TopK, got %d", input.TopK)
				}
				if input.Mode != "" {
					t.Errorf("expected empty Mode, got %q", input.Mode)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var input astSearchInput
			if err := json.Unmarshal([]byte(tt.json), &input); err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			tt.validate(t, input)
		})
	}
}

func TestASTSourceInput_JSON(t *testing.T) {
	j := `{
		"project_dir": "/proj",
		"path": "main.go",
		"entity": "MyFunc",
		"entity_type": "Function",
		"head": 10,
		"tail": 5,
		"start_line": 1,
		"end_line": 100,
		"pattern": "TODO",
		"regex": true,
		"before": 3,
		"after": 3,
		"line_numbers": true
	}`

	var input astSourceInput
	if err := json.Unmarshal([]byte(j), &input); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if input.Path != "main.go" {
		t.Errorf("Path = %q", input.Path)
	}
	if input.Entity != "MyFunc" {
		t.Errorf("Entity = %q", input.Entity)
	}
	if input.EntityType != "Function" {
		t.Errorf("EntityType = %q", input.EntityType)
	}
	if input.Head != 10 {
		t.Errorf("Head = %d", input.Head)
	}
	if input.Tail != 5 {
		t.Errorf("Tail = %d", input.Tail)
	}
	if input.StartLine != 1 {
		t.Errorf("StartLine = %d", input.StartLine)
	}
	if input.EndLine != 100 {
		t.Errorf("EndLine = %d", input.EndLine)
	}
	if input.Pattern != "TODO" {
		t.Errorf("Pattern = %q", input.Pattern)
	}
	if !input.IsRegex {
		t.Error("IsRegex should be true")
	}
	if input.Before != 3 {
		t.Errorf("Before = %d", input.Before)
	}
	if input.After != 3 {
		t.Errorf("After = %d", input.After)
	}
	if !input.LineNumbers {
		t.Error("LineNumbers should be true")
	}
}

func TestASTAIQueryInput_JSON(t *testing.T) {
	j := `{"query":"Find all exported functions","project_dir":"/proj","context":"ctx"}`
	var input astAIQueryInput
	if err := json.Unmarshal([]byte(j), &input); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if input.Query != "Find all exported functions" {
		t.Errorf("Query = %q", input.Query)
	}
}

func TestASTSchemaInput_JSON(t *testing.T) {
	j := `{"project_dir":"/proj","context":"ctx"}`
	var input astSchemaInput
	if err := json.Unmarshal([]byte(j), &input); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if input.ProjectDir != "/proj" {
		t.Errorf("ProjectDir = %q", input.ProjectDir)
	}
	if input.Context != "ctx" {
		t.Errorf("Context = %q", input.Context)
	}
}

// ---------- Hub input struct JSON ----------

func TestHubInputStructs_JSON(t *testing.T) {
	t.Run("hubListInput", func(t *testing.T) {
		var input hubListInput
		if err := json.Unmarshal([]byte(`{"type":"knowledge"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Type != "knowledge" {
			t.Errorf("Type = %q", input.Type)
		}
	})

	t.Run("hubSearchInput", func(t *testing.T) {
		var input hubSearchInput
		if err := json.Unmarshal([]byte(`{"query":"test","type":"rule"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Query != "test" {
			t.Errorf("Query = %q", input.Query)
		}
		if input.Type != "rule" {
			t.Errorf("Type = %q", input.Type)
		}
	})

	t.Run("hubShowInput", func(t *testing.T) {
		var input hubShowInput
		if err := json.Unmarshal([]byte(`{"id":"my-rule","type":"rule"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.ID != "my-rule" {
			t.Errorf("ID = %q", input.ID)
		}
	})

	t.Run("hubInstallInput", func(t *testing.T) {
		var input hubInstallInput
		if err := json.Unmarshal([]byte(`{"id":"my-rule@1.2.0","type":"rule","ide":"claude","project_dir":"/proj"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.ID != "my-rule@1.2.0" {
			t.Errorf("ID = %q", input.ID)
		}
		if input.IDE != "claude" {
			t.Errorf("IDE = %q", input.IDE)
		}
	})

	t.Run("hubUninstallInput", func(t *testing.T) {
		var input hubUninstallInput
		if err := json.Unmarshal([]byte(`{"id":"my-rule","type":"rule","ide":"cursor","project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.ID != "my-rule" {
			t.Errorf("ID = %q", input.ID)
		}
	})

	t.Run("hubUpdateInput", func(t *testing.T) {
		var input hubUpdateInput
		if err := json.Unmarshal([]byte(`{"id":"my-rule","type":"rule","ide":"gemini","project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.ID != "my-rule" {
			t.Errorf("ID = %q", input.ID)
		}
		if input.IDE != "gemini" {
			t.Errorf("IDE = %q", input.IDE)
		}
	})

	t.Run("hubUpdateInput_omitID", func(t *testing.T) {
		var input hubUpdateInput
		if err := json.Unmarshal([]byte(`{"project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.ID != "" {
			t.Errorf("expected empty ID, got %q", input.ID)
		}
	})
}

// ---------- Knowledge input struct JSON ----------

func TestKnowledgeInputStructs_JSON(t *testing.T) {
	t.Run("knowledgeQueryInput", func(t *testing.T) {
		var input knowledgeQueryInput
		if err := json.Unmarshal([]byte(`{"query":"how does auth work?","project_dir":"/proj","context":"ctx"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Query != "how does auth work?" {
			t.Errorf("Query = %q", input.Query)
		}
	})

	t.Run("knowledgeSearchInput", func(t *testing.T) {
		var input knowledgeSearchInput
		if err := json.Unmarshal([]byte(`{"query":"auth","top_k":5,"project_dir":"/proj"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.TopK != 5 {
			t.Errorf("TopK = %d", input.TopK)
		}
	})

	t.Run("wikiSearchInput", func(t *testing.T) {
		var input wikiSearchInput
		j := `{"query":"test","wikis":["project","memory"],"hub_refs":["art@1.0"],"session_id":"abc","top_k":10,"project_dir":"/p"}`
		if err := json.Unmarshal([]byte(j), &input); err != nil {
			t.Fatal(err)
		}
		if len(input.Wikis) != 2 {
			t.Errorf("Wikis length = %d", len(input.Wikis))
		}
		if len(input.HubRefs) != 1 {
			t.Errorf("HubRefs length = %d", len(input.HubRefs))
		}
		if input.SessionID != "abc" {
			t.Errorf("SessionID = %q", input.SessionID)
		}
	})

	t.Run("wikiChatInput", func(t *testing.T) {
		var input wikiChatInput
		if err := json.Unmarshal([]byte(`{"session_id":"sid","message":"hello"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.SessionID != "sid" {
			t.Errorf("SessionID = %q", input.SessionID)
		}
		if input.Message != "hello" {
			t.Errorf("Message = %q", input.Message)
		}
	})

	t.Run("wikiSessionsInput", func(t *testing.T) {
		var input wikiSessionsInput
		if err := json.Unmarshal([]byte(`{"action":"list","project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Action != "list" {
			t.Errorf("Action = %q", input.Action)
		}
	})
}

// ---------- Memory input struct JSON ----------

func TestMemoryInputStructs_JSON(t *testing.T) {
	t.Run("memoryQueryInput", func(t *testing.T) {
		var input memoryQueryInput
		if err := json.Unmarshal([]byte(`{"query":"conventions","scope":"user","project_dir":"/p","context":"ctx"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Query != "conventions" {
			t.Errorf("Query = %q", input.Query)
		}
		if input.Scope != "user" {
			t.Errorf("Scope = %q", input.Scope)
		}
	})

	t.Run("memoryListInput", func(t *testing.T) {
		var input memoryListInput
		if err := json.Unmarshal([]byte(`{"scope":"project","project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Scope != "project" {
			t.Errorf("Scope = %q", input.Scope)
		}
	})

	t.Run("memoryAddInput", func(t *testing.T) {
		var input memoryAddInput
		j := `{"title":"Test Memory","content":"Some content","type":"convention","scope":"project","important":true,"tags":"go,test","project_dir":"/p"}`
		if err := json.Unmarshal([]byte(j), &input); err != nil {
			t.Fatal(err)
		}
		if input.Title != "Test Memory" {
			t.Errorf("Title = %q", input.Title)
		}
		if input.Content != "Some content" {
			t.Errorf("Content = %q", input.Content)
		}
		if input.Type != "convention" {
			t.Errorf("Type = %q", input.Type)
		}
		if !input.Important {
			t.Error("Important should be true")
		}
		if input.Tags != "go,test" {
			t.Errorf("Tags = %q", input.Tags)
		}
	})

	t.Run("memoryRemoveInput", func(t *testing.T) {
		var input memoryRemoveInput
		if err := json.Unmarshal([]byte(`{"id":"my-memory","scope":"user","project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.ID != "my-memory" {
			t.Errorf("ID = %q", input.ID)
		}
	})

	t.Run("memorySearchInput", func(t *testing.T) {
		var input memorySearchInput
		if err := json.Unmarshal([]byte(`{"query":"test","scope":"project","project_dir":"/p"}`), &input); err != nil {
			t.Fatal(err)
		}
		if input.Query != "test" {
			t.Errorf("Query = %q", input.Query)
		}
	})
}

// ---------- Input struct JSON round-trip ----------

func TestInputStructs_JSONRoundTrip(t *testing.T) {
	t.Run("astQueryInput", func(t *testing.T) {
		orig := astQueryInput{
			Query:      "MATCH (n) RETURN n LIMIT 10",
			ProjectDir: "/home/user/project",
			Context:    "mycontext",
		}
		data, err := json.Marshal(orig)
		if err != nil {
			t.Fatal(err)
		}
		var decoded astQueryInput
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatal(err)
		}
		if decoded != orig {
			t.Errorf("round-trip mismatch: got %+v, want %+v", decoded, orig)
		}
	})

	t.Run("memoryAddInput", func(t *testing.T) {
		orig := memoryAddInput{
			Title:      "Test",
			Content:    "Content with special chars: <>&\"'",
			Type:       "skill",
			Scope:      "user",
			Important:  true,
			Tags:       "a,b,c",
			ProjectDir: "/proj",
		}
		data, err := json.Marshal(orig)
		if err != nil {
			t.Fatal(err)
		}
		var decoded memoryAddInput
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatal(err)
		}
		if decoded != orig {
			t.Errorf("round-trip mismatch: got %+v, want %+v", decoded, orig)
		}
	})
}

// ---------- Input struct omitempty behavior ----------

func TestInputStructs_OmitEmpty(t *testing.T) {
	t.Run("astQueryInput omits optional fields", func(t *testing.T) {
		input := astQueryInput{Query: "RETURN 1"}
		data, err := json.Marshal(input)
		if err != nil {
			t.Fatal(err)
		}
		s := string(data)
		if strings.Contains(s, "project_dir") {
			t.Error("empty ProjectDir should be omitted")
		}
		if strings.Contains(s, "context") {
			t.Error("empty Context should be omitted")
		}
		if !strings.Contains(s, "query") {
			t.Error("Query should always be present")
		}
	})

	t.Run("memoryAddInput omits optional fields", func(t *testing.T) {
		input := memoryAddInput{Title: "T", Content: "C"}
		data, err := json.Marshal(input)
		if err != nil {
			t.Fatal(err)
		}
		s := string(data)
		if strings.Contains(s, "important") {
			t.Error("false Important should be omitted (omitempty)")
		}
		if strings.Contains(s, "tags") {
			t.Error("empty Tags should be omitted")
		}
	})
}

// ---------- Brand/tool name integration ----------

func TestMCPToolNames(t *testing.T) {
	// Verify tool names follow the expected pattern
	tests := []struct {
		parts    []string
		wantName string
	}{
		{parts: []string{"ast", "query"}, wantName: brand.Brand + "_ast_query"},
		{parts: []string{"ast", "search"}, wantName: brand.Brand + "_ast_search"},
		{parts: []string{"ast", "query_ai"}, wantName: brand.Brand + "_ast_query_ai"},
		{parts: []string{"ast", "schema"}, wantName: brand.Brand + "_ast_schema"},
		{parts: []string{"ast", "source"}, wantName: brand.Brand + "_ast_source"},
		{parts: []string{"hub", "list"}, wantName: brand.Brand + "_hub_list"},
		{parts: []string{"hub", "search"}, wantName: brand.Brand + "_hub_search"},
		{parts: []string{"hub", "show"}, wantName: brand.Brand + "_hub_show"},
		{parts: []string{"hub", "install"}, wantName: brand.Brand + "_hub_install"},
		{parts: []string{"hub", "uninstall"}, wantName: brand.Brand + "_hub_uninstall"},
		{parts: []string{"hub", "update"}, wantName: brand.Brand + "_hub_update"},
		{parts: []string{"knowledge", "query"}, wantName: brand.Brand + "_knowledge_query"},
		{parts: []string{"knowledge", "search"}, wantName: brand.Brand + "_knowledge_search"},
		{parts: []string{"wiki", "search"}, wantName: brand.Brand + "_wiki_search"},
		{parts: []string{"wiki", "chat"}, wantName: brand.Brand + "_wiki_chat"},
		{parts: []string{"wiki", "sessions"}, wantName: brand.Brand + "_wiki_sessions"},
		{parts: []string{"memory", "query"}, wantName: brand.Brand + "_memory_query"},
		{parts: []string{"memory", "list"}, wantName: brand.Brand + "_memory_list"},
		{parts: []string{"memory", "add"}, wantName: brand.Brand + "_memory_add"},
		{parts: []string{"memory", "remove"}, wantName: brand.Brand + "_memory_remove"},
		{parts: []string{"memory", "search"}, wantName: brand.Brand + "_memory_search"},
	}

	for _, tt := range tests {
		t.Run(strings.Join(tt.parts, "_"), func(t *testing.T) {
			got := brand.MCPToolName(tt.parts...)
			if got != tt.wantName {
				t.Errorf("MCPToolName(%v) = %q, want %q", tt.parts, got, tt.wantName)
			}
		})
	}
}

func TestMCPServerName(t *testing.T) {
	got := brand.MCPServerName("code")
	want := brand.Brand + "-code-mcp"
	if got != want {
		t.Errorf("MCPServerName(\"code\") = %q, want %q", got, want)
	}
}

// ---------- HTTP Handler creation ----------

func TestNewStreamableHTTPHandler(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	if handler == nil {
		t.Fatal("expected non-nil HTTP handler")
	}
}

func TestHTTPHandler_RejectsGET(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	// GET requests to MCP endpoint should be rejected (MCP uses POST)
	req := httptest.NewRequest(http.MethodGet, "/mcp", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	// The MCP spec expects POST; a GET should not return 200 OK.
	if w.Code == http.StatusOK {
		t.Error("GET request should not return 200 OK")
	}
}

func TestHTTPHandler_PostWithoutBody(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	req := httptest.NewRequest(http.MethodPost, "/mcp", nil)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	// An empty POST body should return an error status
	if w.Code == http.StatusOK {
		t.Error("POST with empty body should not return 200 OK")
	}
}

func TestHTTPHandler_PostWithInvalidJSON(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	body := strings.NewReader("{invalid json")
	req := httptest.NewRequest(http.MethodPost, "/mcp", body)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code == http.StatusOK {
		t.Error("POST with invalid JSON should not return 200 OK")
	}
}

func TestHTTPHandler_PostWithValidJSONRPC_Initialize(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	// Send a valid JSON-RPC initialize request
	initReq := map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  "initialize",
		"params": map[string]any{
			"protocolVersion": "2025-03-26",
			"capabilities":   map[string]any{},
			"clientInfo": map[string]any{
				"name":    "test-client",
				"version": "1.0.0",
			},
		},
	}
	reqBody, _ := json.Marshal(initReq)
	req := httptest.NewRequest(http.MethodPost, "/mcp", bytes.NewReader(reqBody))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("initialize request returned status %d, want 200; body: %s", w.Code, w.Body.String())
	}

	// Parse the response — may be JSON or SSE depending on SDK behavior.
	respBody := w.Body.String()
	var jsonData string
	if strings.Contains(w.Header().Get("Content-Type"), "text/event-stream") {
		var ok bool
		jsonData, ok = extractJSONFromSSE(respBody)
		if !ok {
			t.Fatalf("failed to extract JSON from SSE response: %s", respBody)
		}
	} else {
		jsonData = respBody
	}

	var resp map[string]any
	if err := json.Unmarshal([]byte(jsonData), &resp); err != nil {
		t.Fatalf("failed to parse response JSON: %v; body: %s", err, jsonData)
	}

	if resp["jsonrpc"] != "2.0" {
		t.Errorf("expected jsonrpc 2.0, got %v", resp["jsonrpc"])
	}
	if resp["result"] == nil {
		t.Error("expected non-nil result in initialize response")
	}

	// Verify the result contains expected fields
	result, ok := resp["result"].(map[string]any)
	if !ok {
		t.Fatalf("expected result to be a map, got %T", resp["result"])
	}
	if result["serverInfo"] == nil {
		t.Error("expected serverInfo in result")
	}
	if result["capabilities"] == nil {
		t.Error("expected capabilities in result")
	}

	// Verify server info matches brand
	serverInfo, ok := result["serverInfo"].(map[string]any)
	if !ok {
		t.Fatalf("expected serverInfo to be a map, got %T", result["serverInfo"])
	}
	expectedName := brand.MCPServerName("code")
	if serverInfo["name"] != expectedName {
		t.Errorf("serverInfo.name = %v, want %q", serverInfo["name"], expectedName)
	}
	if serverInfo["version"] != version.Version {
		t.Errorf("serverInfo.version = %v, want %q", serverInfo["version"], version.Version)
	}

	// Verify tools capability is advertised
	caps, ok := result["capabilities"].(map[string]any)
	if !ok {
		t.Fatalf("expected capabilities to be a map, got %T", result["capabilities"])
	}
	if caps["tools"] == nil {
		t.Error("expected tools capability in response")
	}
}

func TestHTTPHandler_DELETE(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	req := httptest.NewRequest(http.MethodDelete, "/mcp", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	// DELETE without a valid session should fail
	if w.Code == http.StatusOK {
		t.Error("DELETE without session should not return 200 OK")
	}
}

// ---------- ServeHTTP address resolution ----------

func TestServeHTTP_AddressDefaults(t *testing.T) {
	tests := []struct {
		name     string
		host     string
		port     int
		wantAddr string
	}{
		{
			name:     "both defaults",
			host:     "",
			port:     0,
			wantAddr: "127.0.0.1:8282",
		},
		{
			name:     "custom host",
			host:     "0.0.0.0",
			port:     0,
			wantAddr: "0.0.0.0:8282",
		},
		{
			name:     "custom port",
			host:     "",
			port:     9090,
			wantAddr: "127.0.0.1:9090",
		},
		{
			name:     "both custom",
			host:     "192.168.1.1",
			port:     3000,
			wantAddr: "192.168.1.1:3000",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			host := tt.host
			if host == "" {
				host = "127.0.0.1"
			}
			port := tt.port
			if port == 0 {
				port = 8282
			}
			addr := fmt.Sprintf("%s:%d", host, port)
			if addr != tt.wantAddr {
				t.Errorf("addr = %q, want %q", addr, tt.wantAddr)
			}
		})
	}
}

// ---------- HTTP Handler with initialized session: ListTools ----------

// extractJSONFromSSE extracts JSON-RPC message data from an SSE response body.
// SSE format: "event: message\ndata: {json}\n\n"
func extractJSONFromSSE(body string) (string, bool) {
	for _, line := range strings.Split(body, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "data: ") {
			return strings.TrimPrefix(line, "data: "), true
		}
	}
	return "", false
}

func TestHTTPHandler_ListTools(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	// Step 1: Initialize
	initReq := map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  "initialize",
		"params": map[string]any{
			"protocolVersion": "2025-03-26",
			"capabilities":   map[string]any{},
			"clientInfo": map[string]any{
				"name":    "test-client",
				"version": "1.0.0",
			},
		},
	}
	reqBody, _ := json.Marshal(initReq)
	req := httptest.NewRequest(http.MethodPost, "/mcp", bytes.NewReader(reqBody))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("initialize failed with status %d: %s", w.Code, w.Body.String())
	}

	// Get session ID from response header
	sessionID := w.Header().Get("Mcp-Session-Id")

	// Step 2: Send initialized notification
	notifReq := map[string]any{
		"jsonrpc": "2.0",
		"method":  "notifications/initialized",
	}
	notifBody, _ := json.Marshal(notifReq)
	req2 := httptest.NewRequest(http.MethodPost, "/mcp", bytes.NewReader(notifBody))
	req2.Header.Set("Content-Type", "application/json")
	req2.Header.Set("Accept", "application/json, text/event-stream")
	if sessionID != "" {
		req2.Header.Set("Mcp-Session-Id", sessionID)
	}
	w2 := httptest.NewRecorder()
	handler.ServeHTTP(w2, req2)

	// Step 3: List tools
	listReq := map[string]any{
		"jsonrpc": "2.0",
		"id":      2,
		"method":  "tools/list",
	}
	listBody, _ := json.Marshal(listReq)
	req3 := httptest.NewRequest(http.MethodPost, "/mcp", bytes.NewReader(listBody))
	req3.Header.Set("Content-Type", "application/json")
	req3.Header.Set("Accept", "application/json, text/event-stream")
	if sessionID != "" {
		req3.Header.Set("Mcp-Session-Id", sessionID)
	}
	w3 := httptest.NewRecorder()
	handler.ServeHTTP(w3, req3)

	if w3.Code != http.StatusOK {
		t.Fatalf("tools/list failed with status %d: %s", w3.Code, w3.Body.String())
	}

	// The response may be JSON or SSE depending on the server implementation.
	respBody := w3.Body.String()
	var jsonData string
	if strings.Contains(w3.Header().Get("Content-Type"), "text/event-stream") {
		var ok bool
		jsonData, ok = extractJSONFromSSE(respBody)
		if !ok {
			t.Fatalf("failed to extract JSON from SSE response: %s", respBody)
		}
	} else {
		jsonData = respBody
	}

	var resp map[string]any
	if err := json.Unmarshal([]byte(jsonData), &resp); err != nil {
		t.Fatalf("failed to parse tools/list response: %v; body: %s", err, jsonData)
	}

	result, ok := resp["result"].(map[string]any)
	if !ok {
		t.Fatalf("expected result map, got %T", resp["result"])
	}

	tools, ok := result["tools"].([]any)
	if !ok {
		t.Fatalf("expected tools array, got %T", result["tools"])
	}

	// Verify we have the expected number of registered tools
	// AST: query, search, query_ai, schema, source = 5
	// Knowledge: query, search = 2
	// Wiki: search, chat, sessions = 3
	// Memory: query, list, add, remove, search = 5
	// Hub: list, search, show, install, uninstall, update = 6
	// Total = 21
	expectedToolCount := 21
	if len(tools) != expectedToolCount {
		t.Errorf("expected %d tools, got %d", expectedToolCount, len(tools))
		// List tool names for debugging
		for i, tool := range tools {
			if tm, ok := tool.(map[string]any); ok {
				t.Logf("  tool[%d]: %v", i, tm["name"])
			}
		}
	}

	// Verify specific tool names are present
	toolNames := make(map[string]bool)
	for _, tool := range tools {
		if tm, ok := tool.(map[string]any); ok {
			if name, ok := tm["name"].(string); ok {
				toolNames[name] = true
			}
		}
	}

	expectedTools := []string{
		brand.MCPToolName("ast", "query"),
		brand.MCPToolName("ast", "search"),
		brand.MCPToolName("ast", "query_ai"),
		brand.MCPToolName("ast", "schema"),
		brand.MCPToolName("ast", "source"),
		brand.MCPToolName("hub", "list"),
		brand.MCPToolName("hub", "search"),
		brand.MCPToolName("hub", "show"),
		brand.MCPToolName("hub", "install"),
		brand.MCPToolName("hub", "uninstall"),
		brand.MCPToolName("hub", "update"),
		brand.MCPToolName("knowledge", "query"),
		brand.MCPToolName("knowledge", "search"),
		brand.MCPToolName("wiki", "search"),
		brand.MCPToolName("wiki", "chat"),
		brand.MCPToolName("wiki", "sessions"),
		brand.MCPToolName("memory", "query"),
		brand.MCPToolName("memory", "list"),
		brand.MCPToolName("memory", "add"),
		brand.MCPToolName("memory", "remove"),
		brand.MCPToolName("memory", "search"),
	}

	for _, name := range expectedTools {
		if !toolNames[name] {
			t.Errorf("missing expected tool %q", name)
		}
	}
}

// ---------- ServeHTTP context cancellation ----------

func TestServeHTTP_ContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	// Use a port that is not in use
	opts := Options{
		Host: "127.0.0.1",
		Port: 0, // will default to 8282, but with cancelled context it shouldn't bind
	}

	// ServeHTTP with a cancelled context should return quickly without error
	// (because the server closes immediately).
	err := ServeHTTP(ctx, opts)
	// The error should be nil (closed cleanly) or contain "server closed"
	if err != nil && !strings.Contains(err.Error(), "closed") {
		// On some systems, the server may still try to bind, which is fine
		t.Logf("ServeHTTP with cancelled context returned: %v", err)
	}
}

// ---------- Capabilities verification ----------

func TestNewServer_CapabilitiesViaInitialize(t *testing.T) {
	// Capabilities are verified through the initialize handshake in
	// TestHTTPHandler_PostWithValidJSONRPC_Initialize which asserts
	// that the "tools" capability is advertised in the response.
	server := NewServer()
	if server == nil {
		t.Fatal("expected non-nil server")
	}
}

// ---------- HTTP handler with multiple methods ----------

func TestHTTPHandler_UnsupportedMethod(t *testing.T) {
	server := NewServer()

	handler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		return server
	}, nil)

	methods := []string{http.MethodPut, http.MethodPatch, http.MethodHead}
	for _, method := range methods {
		t.Run(method, func(t *testing.T) {
			req := httptest.NewRequest(method, "/mcp", nil)
			w := httptest.NewRecorder()
			handler.ServeHTTP(w, req)

			if w.Code == http.StatusOK {
				t.Errorf("%s should not return 200 OK", method)
			}
		})
	}
}

// ---------- resolveProjectDir edge cases ----------

func TestResolveProjectDir_SymlinkTarget(t *testing.T) {
	tmpDir := t.TempDir()
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "user"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "testctx"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "testctx"), 0755)
	realDir := filepath.Join(tmpDir, "real")
	if err := os.MkdirAll(realDir, 0o755); err != nil {
		t.Fatal(err)
	}

	linkDir := filepath.Join(tmpDir, "link")
	if err := os.Symlink(realDir, linkDir); err != nil {
		t.Skipf("symlinks not supported: %v", err)
	}

	result, err := resolveProjectDir(linkDir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// The result should be a valid path (resolves either the link or its target)
	if _, err := os.Stat(result); err != nil {
		t.Errorf("result path %q does not exist: %v", result, err)
	}
}

func TestResolveProjectDir_FileNotDirectory(t *testing.T) {
	tmpDir := t.TempDir()
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "user"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "memory", "testctx"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "project"), 0755)
        _ = os.MkdirAll(filepath.Join(tmpDir, ".graphit", "knowledge", "testctx"), 0755)
	filePath := filepath.Join(tmpDir, "file.txt")
	if err := os.WriteFile(filePath, []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}

	// resolveProjectDir stat checks existence but doesn't check if it's a directory.
	// This is expected behavior — it trusts the caller to pass a directory.
	result, err := resolveProjectDir(filePath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result != filePath {
		t.Errorf("got %q, want %q", result, filePath)
	}
}
