package agent

import (
	"strings"
	"testing"

	"github.com/pelletier/go-toml/v2"
	"gopkg.in/yaml.v3"
)

func readOnlyDescriptor() AgentDescriptor {
	return AgentDescriptor{
		Name:        "graphit-scout",
		Description: "Recall and locate: answer from Memory, Task, Knowledge, AST and Hub without changing anything.",
		ReadOnly:    true,
		Body:        "You perform the scout role.\n\nAnswer with verifiable identifiers.",
	}
}

// One descriptor has to produce a file each host actually parses. A field the
// host does not know is ignored silently, which is why each expectation below
// is the vocabulary that host documents.
func TestAgentDocumentSpeaksEachHostDialect(t *testing.T) {
	t.Parallel()

	for _, tc := range []struct {
		agent    string
		wantExt  string
		wantKeys []string
		denyKeys []string
	}{
		{agent: "claude", wantExt: ".md", wantKeys: []string{"name", "description"}},
		{agent: "qwen", wantExt: ".md", wantKeys: []string{"name", "description"}},
		{agent: "kimi", wantExt: ".md", wantKeys: []string{"name", "description"}},
		{agent: "cursor", wantExt: ".md", wantKeys: []string{"name", "description", "readonly"}},
		{agent: "gemini", wantExt: ".md", wantKeys: []string{"name", "description", "kind", "tools"}},
		{agent: "antigravity", wantExt: ".md", wantKeys: []string{"name", "description", "subagent"}},
		{agent: "kiro", wantExt: ".md", wantKeys: []string{"name", "description", "tools"}},
		{agent: "opencode", wantExt: ".md", wantKeys: []string{"description", "mode", "permission"}, denyKeys: []string{"name"}},
	} {
		t.Run(tc.agent, func(t *testing.T) {
			t.Parallel()
			fileName, content, err := AgentDocument(tc.agent, readOnlyDescriptor())
			if err != nil {
				t.Fatal(err)
			}
			if !strings.HasSuffix(fileName, tc.wantExt) {
				t.Fatalf("%s file name = %q, want %s", tc.agent, fileName, tc.wantExt)
			}
			front, body := splitFrontmatter(t, content)
			for _, key := range tc.wantKeys {
				if _, ok := front[key]; !ok {
					t.Errorf("%s frontmatter is missing %q: %v", tc.agent, key, front)
				}
			}
			for _, key := range tc.denyKeys {
				if _, ok := front[key]; ok {
					t.Errorf("%s frontmatter must not carry %q: %v", tc.agent, key, front)
				}
			}
			if !strings.Contains(body, "You perform the scout role.") {
				t.Errorf("%s lost the role body: %q", tc.agent, body)
			}
		})
	}
}

// Codex is the odd one out: standalone TOML, with the instructions in a field
// rather than after a frontmatter delimiter.
func TestAgentDocumentRendersCodexAsTOML(t *testing.T) {
	t.Parallel()

	fileName, content, err := AgentDocument("codex", readOnlyDescriptor())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(fileName, ".toml") {
		t.Fatalf("Codex agent file = %q, want a .toml file", fileName)
	}
	if strings.HasPrefix(content, "---") {
		t.Fatalf("Codex does not read frontmatter: %s", content)
	}
	var document map[string]any
	if err := toml.Unmarshal([]byte(content), &document); err != nil {
		t.Fatalf("Codex agent is not valid TOML: %v\n%s", err, content)
	}
	for _, key := range []string{"name", "description", "developer_instructions"} {
		if _, ok := document[key]; !ok {
			t.Errorf("Codex agent is missing required key %q: %v", key, document)
		}
	}
	instructions, _ := document["developer_instructions"].(string)
	if !strings.Contains(instructions, "You perform the scout role.") {
		t.Errorf("Codex agent lost the role body: %q", instructions)
	}
	if document["sandbox_mode"] != "read-only" {
		t.Errorf("a read-only role should ask Codex for its read-only sandbox: %v", document["sandbox_mode"])
	}
}

// A colon in the description is what breaks a hand-built YAML block; the whole
// reason the frontmatter is marshalled instead of concatenated.
func TestAgentDocumentSurvivesAColonInTheDescription(t *testing.T) {
	t.Parallel()

	descriptor := readOnlyDescriptor()
	descriptor.Description = "Recall: answer from Memory, Task and AST"

	for _, agentName := range []string{"claude", "opencode", "gemini", "cursor", "kiro", "antigravity", "qwen", "kimi"} {
		_, content, err := AgentDocument(agentName, descriptor)
		if err != nil {
			t.Fatalf("%s: %v", agentName, err)
		}
		front, _ := splitFrontmatter(t, content)
		if front["description"] != descriptor.Description {
			t.Errorf("%s did not round-trip the description: %v", agentName, front["description"])
		}
	}
}

// Where a host cannot express a restriction, nothing may be invented: an
// unknown key is ignored silently and reads as a guarantee that is not there.
func TestAgentDocumentDoesNotFabricateRestrictions(t *testing.T) {
	t.Parallel()

	_, content, err := AgentDocument("cursor", readOnlyDescriptor())
	if err != nil {
		t.Fatal(err)
	}
	front, _ := splitFrontmatter(t, content)
	if _, ok := front["tools"]; ok {
		t.Errorf("Cursor has no tool allowlist; emitting one pretends at enforcement: %v", front)
	}
	if front["readonly"] != true {
		t.Errorf("Cursor does have readonly and it must be used: %v", front)
	}

	// Gemini is the opposite case: omitting tools leaves the role with none, so
	// the allowlist is required and must reach MCP through its own wildcard.
	_, geminiContent, err := AgentDocument("gemini", readOnlyDescriptor())
	if err != nil {
		t.Fatal(err)
	}
	geminiFront, _ := splitFrontmatter(t, geminiContent)
	tools, _ := geminiFront["tools"].([]any)
	var joined []string
	for _, tool := range tools {
		joined = append(joined, tool.(string))
	}
	if !contains(joined, "mcp_*") {
		t.Errorf("Gemini needs its own MCP wildcard: %v", joined)
	}
	if contains(joined, "mcp__*") {
		t.Errorf("Gemini uses mcp_server_tool, not the Claude namespacing: %v", joined)
	}
	if contains(joined, "write_file") || contains(joined, "run_shell_command") {
		t.Errorf("a read-only role must not be granted write or shell: %v", joined)
	}
}

func TestAgentDocumentGrantsShellOnlyWhenTheRoleNeedsIt(t *testing.T) {
	t.Parallel()

	tracker := readOnlyDescriptor()
	tracker.Name = "graphit-tracker"
	tracker.AllowsShell = true

	_, content, err := AgentDocument("gemini", tracker)
	if err != nil {
		t.Fatal(err)
	}
	front, _ := splitFrontmatter(t, content)
	tools, _ := front["tools"].([]any)
	found := false
	for _, tool := range tools {
		if tool == "run_shell_command" {
			found = true
		}
		if tool == "write_file" {
			t.Errorf("shell access must not imply write access: %v", tools)
		}
	}
	if !found {
		t.Errorf("a role that needs a diff must be allowed to run one: %v", tools)
	}

	_, openCodeContent, err := AgentDocument("opencode", tracker)
	if err != nil {
		t.Fatal(err)
	}
	openCodeFront, _ := splitFrontmatter(t, openCodeContent)
	permission, _ := openCodeFront["permission"].(map[string]any)
	if permission["edit"] != "deny" {
		t.Errorf("a read-only role must still be denied edits: %v", permission)
	}
	if _, denied := permission["bash"]; denied {
		t.Errorf("a role that needs a diff must not have bash denied: %v", permission)
	}
}

func splitFrontmatter(t *testing.T, content string) (map[string]any, string) {
	t.Helper()
	if !strings.HasPrefix(content, "---\n") {
		t.Fatalf("document has no frontmatter: %s", content)
	}
	rest := strings.TrimPrefix(content, "---\n")
	end := strings.Index(rest, "\n---\n")
	if end < 0 {
		t.Fatalf("frontmatter is not terminated: %s", content)
	}
	var front map[string]any
	if err := yaml.Unmarshal([]byte(rest[:end+1]), &front); err != nil {
		t.Fatalf("frontmatter is not valid YAML: %v\n%s", err, content)
	}
	return front, rest[end+len("\n---\n"):]
}

func contains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
