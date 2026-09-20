package agent

import (
	"fmt"
	"strings"

	"github.com/pelletier/go-toml/v2"
	"gopkg.in/yaml.v3"
)

// AgentDescriptor is the host-neutral definition of a delegated role. It says
// what the role is and what it is allowed to touch; translating that into a
// host's vocabulary is this file's job.
//
// It deliberately carries capabilities, not tool names: the vocabularies differ
// per host, and a name invented for one host is silently ignored by another.
type AgentDescriptor struct {
	Name        string
	Description string
	// ReadOnly roles must not change anything. Hosts that can enforce this are
	// told to; on the rest it stays an instruction in Body.
	ReadOnly bool
	// AllowsShell is separate from ReadOnly because assessing impact needs to
	// run a diff without being allowed to edit.
	AllowsShell bool
	// Body is the role document. On most hosts it is the markdown after the
	// frontmatter; Codex carries it in a TOML field instead.
	Body string
}

// AgentDocument renders the file a host reads for one delegated role, returning
// the file name and its full contents.
//
// Only fields whose meaning is documented by the host are emitted. Where a host
// cannot express a restriction, nothing is fabricated: the restriction stays in
// the body as an instruction, and the gap is a known limitation rather than a
// pretend guarantee.
func AgentDocument(agentName string, descriptor AgentDescriptor) (string, string, error) {
	if strings.TrimSpace(descriptor.Name) == "" {
		return "", "", fmt.Errorf("agent descriptor has no name")
	}
	switch agentName {
	case "codex":
		return codexAgentDocument(descriptor)
	default:
		return markdownAgentDocument(agentName, descriptor)
	}
}

// codexAgentDocument is the one host that is not frontmatter plus body: Codex
// reads standalone TOML from .codex/agents/ and carries the instructions in
// developer_instructions, alongside the required name and description.
func codexAgentDocument(descriptor AgentDescriptor) (string, string, error) {
	document := map[string]any{
		"name":                   descriptor.Name,
		"description":            descriptor.Description,
		"developer_instructions": descriptor.Body,
	}
	if descriptor.ReadOnly {
		// read-only is the sandbox mode Codex documents for agents that must not
		// change the workspace.
		document["sandbox_mode"] = "read-only"
	}
	encoded, err := toml.Marshal(document)
	if err != nil {
		return "", "", fmt.Errorf("encoding Codex agent %s: %w", descriptor.Name, err)
	}
	return descriptor.Name + ".toml", "# " + managedAgentMarker + "\n" + string(encoded), nil
}

func markdownAgentDocument(agentName string, descriptor AgentDescriptor) (string, string, error) {
	front := agentFrontmatter(agentName, descriptor)
	// Marshal rather than concatenate: a description containing a colon breaks a
	// hand-built YAML block, which is the same trap SkillFrontmatter documents.
	encoded, err := yaml.Marshal(front)
	if err != nil {
		return "", "", fmt.Errorf("encoding %s agent %s: %w", agentName, descriptor.Name, err)
	}
	body := strings.TrimSpace(descriptor.Body)
	// The marker is what makes removal safe: only files carrying it are deleted,
	// so a role the user rewrote by hand is never thrown away.
	return descriptor.Name + ".md", "---\n" + string(encoded) + "---\n\n<!-- " + managedAgentMarker + " -->\n\n" + body + "\n", nil
}

// agentFrontmatter maps the descriptor onto each host's documented keys. Every
// branch below corresponds to fields that host documents; hosts with no branch
// of their own accept the shared name/description pair.
func agentFrontmatter(agentName string, descriptor AgentDescriptor) map[string]any {
	front := map[string]any{
		"name":        descriptor.Name,
		"description": descriptor.Description,
	}
	switch agentName {
	case "opencode":
		// OpenCode identifies an agent by its file name and rejects routing
		// without mode: subagent. Its permission rules are the enforcement it
		// does offer.
		delete(front, "name")
		front["mode"] = "subagent"
		if descriptor.ReadOnly {
			permission := map[string]any{"edit": "deny"}
			if !descriptor.AllowsShell {
				permission["bash"] = "deny"
			}
			front["permission"] = permission
		}

	case "cursor":
		// Cursor has no tool allowlist, but readonly is a real switch.
		if descriptor.ReadOnly {
			front["readonly"] = true
		}

	case "gemini":
		// Gemini requires an explicit allowlist: omitting tools leaves the role
		// with none. mcp_* is its documented wildcard for every MCP tool.
		front["kind"] = "local"
		front["tools"] = geminiAgentTools(descriptor)

	case "antigravity":
		front["subagent"] = true

	case "kiro":
		front["tools"] = kiroAgentTools(descriptor)
	}
	return front
}

func geminiAgentTools(descriptor AgentDescriptor) []string {
	tools := []string{"mcp_*"}
	if !descriptor.ReadOnly {
		return tools
	}
	tools = append(tools, "read_file", "list_directory", "search_file_content")
	if descriptor.AllowsShell {
		tools = append(tools, "run_shell_command")
	}
	return tools
}

// kiroAgentTools uses Kiro's capability categories rather than tool names.
func kiroAgentTools(descriptor AgentDescriptor) []string {
	tools := []string{"read"}
	if descriptor.AllowsShell {
		tools = append(tools, "shell")
	}
	if !descriptor.ReadOnly {
		tools = append(tools, "write")
	}
	return tools
}
