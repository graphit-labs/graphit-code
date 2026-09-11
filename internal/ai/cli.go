package ai

import (
	"context"
	"fmt"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type cliClient struct {
	executablePath string
	binaryName     string
	agentArgs      []string
}

type inputMode int

const (
	inputStdin inputMode = iota
	inputArg
)

// nonInteractivePreamble is prepended to the system prompt so that the AI agent
// knows it MUST NOT attempt interactive actions (tool calls that block waiting
// for human approval, clarifying questions, TUI interactions, etc.).
// This replaces dangerous permission-bypass flags (--yolo, --dangerously-skip-permissions)
// with a prompt-level instruction that keeps the agent sandboxed.
const nonInteractivePreamble = `You are running in non-interactive, autonomous mode.
Constraints you MUST follow:
- Do NOT ask the user any questions or request clarification.
- Do NOT execute actions that require user approval (file edits, shell commands, etc.).
- Do NOT attempt to open a TUI or interactive interface.
- Respond directly with your analysis, output, or answer as plain text.
- If you cannot complete a task without user interaction, explain what you would need instead of attempting it.

`

type cliSpec struct {
	mode           inputMode
	stdinArgs      []string
	argArgs        []string
	sessionFlag    string
	newSessionFlag string
	resumeCommand  []string
}

var knownCLIs = map[string]cliSpec{
	"claude": {
		mode:           inputStdin,
		stdinArgs:      []string{"-p", "-"},
		sessionFlag:    "--resume",
		newSessionFlag: "--session-id",
	},
	"gemini": {
		mode:           inputStdin,
		stdinArgs:      []string{"-p", "-"},
		sessionFlag:    "--resume",
		newSessionFlag: "--session-id",
	},
	"agy": {
		mode:        inputStdin,
		stdinArgs:   []string{"-p", "-"},
		sessionFlag: "--conversation",
	},
	"grok": {
		mode:      inputStdin,
		stdinArgs: []string{"-p", "-"},
	},
	"cursor-agent": {
		mode:      inputStdin,
		stdinArgs: []string{"-p", "-"},
	},
	"codex": {
		mode:          inputStdin,
		stdinArgs:     []string{"exec", "-"},
		resumeCommand: []string{"exec", "resume"},
	},
	"opencode": {
		mode:        inputArg,
		argArgs:     []string{"run"},
		sessionFlag: "-s",
	},
	"kiro-cli": {
		mode:        inputStdin,
		stdinArgs:   []string{"chat", "--no-interactive", "-"},
		sessionFlag: "--resume-id",
	},
	"copilot": {
		mode:        inputStdin,
		stdinArgs:   []string{"-p", "-"},
		sessionFlag: "--resume",
	},
	"qwen": {
		mode:           inputArg,
		argArgs:        []string{"-p"},
		sessionFlag:    "--resume",
		newSessionFlag: "--session-id",
	},
	"kimi": {
		mode:        inputArg,
		argArgs:     []string{"-p"},
		sessionFlag: "--session",
	},
	"deepcode": {
		mode:        inputArg,
		argArgs:     []string{"-p"},
		sessionFlag: "-r",
	},
}

// removedCLIs prevents an explicitly configured legacy name from falling
// through the generic custom-CLI path.
var removedCLIs = map[string]struct{}{
	"agent":     {},
	"cline":     {},
	"deepseek":  {},
	"goose":     {},
	"openhands": {},
}

func specForBinary(name string) cliSpec {
	if spec, ok := knownCLIs[name]; ok {
		return spec
	}
	return cliSpec{mode: inputStdin, stdinArgs: []string{"-"}}
}

func (c *cliClient) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	resp, _, err := c.completeInternalInDirSession(ctx, "", "", systemPrompt, userPrompt, false)
	return resp, err
}

// CompleteInDir runs the configured agent with the requested project as its
// working directory, so project-scoped rules and context match the request.
func (c *cliClient) CompleteInDir(ctx context.Context, workingDir, systemPrompt, userPrompt string) (string, error) {
	resp, _, err := c.completeInternalInDirSession(ctx, workingDir, "", systemPrompt, userPrompt, false)
	return resp, err
}

func (c *cliClient) CompleteWithSession(ctx context.Context, sessionID, systemPrompt, userPrompt string) (string, string, error) {
	return c.completeInternalInDirSession(ctx, "", sessionID, systemPrompt, userPrompt, true)
}

func (c *cliClient) CompleteWithSessionInDir(ctx context.Context, workingDir, sessionID, systemPrompt, userPrompt string) (string, string, error) {
	return c.completeInternalInDirSession(ctx, workingDir, sessionID, systemPrompt, userPrompt, true)
}

func (c *cliClient) SupportsSession() bool {
	spec := specForBinary(c.binaryName)
	if spec.newSessionFlag != "" {
		return true
	}
	stream, structured := streamSpecFor(c.binaryName)
	return structured && stream.capturesSession && (spec.sessionFlag != "" || len(spec.resumeCommand) > 0)
}

func (c *cliClient) AgentCLI() string { return c.binaryName }

func (c *cliClient) completeInternalInDirSession(ctx context.Context, workingDir, sessionID, systemPrompt, userPrompt string, persistSession bool) (string, string, error) {
	result, err := c.CompleteStream(ctx, StreamRequest{
		SystemPrompt:   systemPrompt,
		UserPrompt:     userPrompt,
		SessionID:      sessionID,
		PersistSession: persistSession,
		WorkDir:        workingDir,
	}, nil)
	if result == nil {
		return "", "", fmt.Errorf("CLI fallback %q failed: %w", c.binaryName, err)
	}
	if err != nil {
		return result.Text, result.SessionID, fmt.Errorf("CLI fallback %q failed: %w", c.binaryName, err)
	}
	return result.Text, result.SessionID, nil
}

var agentToCLI = config.CLIForAgent

func tryFallbackCLI(provider string, userCLI string) Client {
	var defaultCandidates []string

	switch provider {
	case "google":
		defaultCandidates = []string{"agy", "gemini", "qwen", "kiro-cli", "cursor-agent", "opencode", "copilot"}
	case "alibaba", "qwen":
		defaultCandidates = []string{"qwen", "opencode", "cursor-agent", "kiro-cli", "copilot"}
	case "moonshot", "kimi":
		defaultCandidates = []string{"kimi", "opencode", "cursor-agent", "kiro-cli", "copilot"}
	case "deepseek":
		defaultCandidates = []string{"deepcode", "opencode", "cursor-agent", "kiro-cli", "copilot"}
	case "anthropic":
		defaultCandidates = []string{"claude", "kiro-cli", "cursor-agent", "opencode", "copilot"}
	case "openai":
		defaultCandidates = []string{"codex", "cursor-agent", "opencode", "kiro-cli", "copilot"}
	case "xai":
		defaultCandidates = []string{"grok", "cursor-agent", "opencode", "kiro-cli", "copilot"}
	case "amazon", "aws":
		defaultCandidates = []string{"kiro-cli", "cursor-agent", "opencode", "gemini", "copilot"}
	default:

		defaultCandidates = []string{"opencode", "agy", "gemini", "claude", "codex", "qwen", "kimi", "deepcode", "grok", "kiro-cli", "cursor-agent", "copilot"}
	}

	var candidates []string

	if userCLI != "" {
		candidates = append(candidates, userCLI)
	}

	agent := config.DefaultAgent()
	if equivalent := agentToCLI(agent); equivalent != "" && equivalent != userCLI {
		candidates = append(candidates, equivalent)
	}

	for _, cand := range defaultCandidates {
		if cand != userCLI && cand != agentToCLI(agent) {
			candidates = append(candidates, cand)
		}
	}

	for _, bin := range candidates {
		if _, removed := removedCLIs[bin]; removed {
			continue
		}
		if path, err := exec.LookPath(bin); err == nil {
			return &cliClient{
				executablePath: path,
				binaryName:     bin,
				agentArgs:      agentArgsFromConfig(bin),
			}
		}
	}

	return nil
}

func agentArgsFromConfig(binary string) []string {
	cfg, err := config.LoadGlobalConfig()
	if err != nil {
		return nil
	}
	if binary != "" {
		if v, _ := config.GetConfigValue(cfg, "ai.agent_args."+binary); strings.TrimSpace(v) != "" {
			return strings.Fields(v)
		}
	}
	if v, _ := config.GetConfigValue(cfg, "ai.agent_args"); strings.TrimSpace(v) != "" {
		return strings.Fields(v)
	}
	return nil
}
