package ai

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type cliClient struct {
	executablePath string
	binaryName     string
	// agentArgs are operator-configured arguments added only to agentic runs,
	// typically the flag that lets the CLI edit files without prompting.
	// See agentArgsFromConfig for the keys.
	agentArgs []string
}

// inputMode determines how the prompt is delivered to the CLI.
type inputMode int

const (
	// inputStdin delivers the prompt via stdin pipe (most secure).
	inputStdin inputMode = iota
	// inputFile writes the prompt to a temp file and passes the path via a flag.
	inputFile
	// inputArg passes the prompt as a positional command-line argument (least secure).
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

// cliSpec defines how to invoke a specific CLI binary in non-interactive mode.
type cliSpec struct {
	// mode determines how the prompt is delivered.
	mode inputMode
	// stdinArgs are the arguments when delivering prompt via stdin.
	// Only used when mode == inputStdin.
	stdinArgs []string
	// fileFlag is the flag before the temp file path (e.g. "-i", "-f").
	// Only used when mode == inputFile.
	fileFlag string
	// fileArgs are any arguments that precede the fileFlag.
	// Only used when mode == inputFile.
	fileArgs []string
	// argArgs are arguments that precede the prompt text.
	// Only used when mode == inputArg.
	argArgs []string
	// sessionFlag is the flag name for resuming a conversation.
	// Empty means the CLI does not support session continuity.
	sessionFlag string
}

// knownCLIs maps binary names to their invocation spec.
// Non-interactive behavior is achieved via the CLI's headless/print mode flag
// (e.g. -p, exec, run, --headless) combined with the nonInteractivePreamble
// in the prompt — NOT via permission-bypass flags like --yolo.
var knownCLIs = map[string]cliSpec{
	"claude": {
		mode:        inputStdin,
		stdinArgs:   []string{"-p", "-"},
		sessionFlag: "--resume",
	},
	"gemini": {
		mode:        inputStdin,
		stdinArgs:   []string{"-p", "-"},
		sessionFlag: "--resume",
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
	"agent": {
		mode:      inputStdin,
		stdinArgs: []string{"-p", "-"},
	},
	"codex": {
		mode:      inputStdin,
		stdinArgs: []string{"exec", "-"},
	},
	"opencode": {
		mode:        inputArg,
		argArgs:     []string{"run"},
		sessionFlag: "-s",
	},
	"kiro-cli": {
		mode:      inputStdin,
		stdinArgs: []string{"chat", "--no-interactive", "-"},
	},
	"copilot": {
		mode:        inputStdin,
		stdinArgs:   []string{"-p", "-"},
		sessionFlag: "--resume",
	},
	"cline": {
		mode:    inputArg,
		argArgs: []string{},
	},
	"goose": {
		mode:     inputFile,
		fileArgs: []string{"run"},
		fileFlag: "-i",
	},
	"openhands": {
		mode:     inputFile,
		fileArgs: []string{"--headless"},
		fileFlag: "-f",
	},
}

func specForBinary(name string) cliSpec {
	if spec, ok := knownCLIs[name]; ok {
		return spec
	}
	return cliSpec{mode: inputStdin, stdinArgs: []string{"-"}}
}

func (c *cliClient) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	resp, _, err := c.completeInternal(ctx, "", systemPrompt, userPrompt)
	return resp, err
}

func (c *cliClient) CompleteWithSession(ctx context.Context, sessionID, systemPrompt, userPrompt string) (string, string, error) {
	return c.completeInternal(ctx, sessionID, systemPrompt, userPrompt)
}

func (c *cliClient) SupportsSession() bool {
	return specForBinary(c.binaryName).sessionFlag != ""
}

func (c *cliClient) completeInternal(ctx context.Context, sessionID, systemPrompt, userPrompt string) (string, string, error) {
	var promptBuilder strings.Builder
	promptBuilder.WriteString(nonInteractivePreamble)
	if systemPrompt != "" {
		promptBuilder.WriteString(systemPrompt)
		promptBuilder.WriteString("\n\n")
	}
	promptBuilder.WriteString(userPrompt)

	prompt := promptBuilder.String()
	spec := specForBinary(c.binaryName)

	var args []string

	if sessionID != "" && spec.sessionFlag != "" {
		args = append(args, spec.sessionFlag, sessionID)
	}

	var cleanupFn func()

	switch spec.mode {
	case inputStdin:
		args = append(args, spec.stdinArgs...)

	case inputFile:
		tmpFile, err := writeTempPrompt(prompt)
		if err != nil {
			return "", "", fmt.Errorf("writing temp prompt for %q: %w", c.binaryName, err)
		}
		cleanupFn = func() { os.Remove(tmpFile) }
		args = append(args, spec.fileArgs...)
		args = append(args, spec.fileFlag, tmpFile)

	case inputArg:
		args = append(args, spec.argArgs...)
		args = append(args, prompt)
	}

	cmd := exec.CommandContext(ctx, c.executablePath, args...)

	var outBuf, errBuf bytes.Buffer
	cmd.Stdout = &outBuf
	cmd.Stderr = &errBuf

	cmd.Env = append(os.Environ(), "NO_COLOR=1", "TERM=dumb")

	switch spec.mode {
	case inputStdin:
		cmd.Stdin = strings.NewReader(prompt)
	case inputArg, inputFile:
		// No stdin needed; prevent deadlocks by providing empty reader
		cmd.Stdin = strings.NewReader("")
	}

	err := cmd.Run()
	if cleanupFn != nil {
		cleanupFn()
	}
	if err != nil {
		return "", "", fmt.Errorf("CLI fallback %q failed: %w (stderr: %s)", c.binaryName, err, errBuf.String())
	}

	response := strings.TrimSpace(outBuf.String())

	returnedSessionID := sessionID

	return response, returnedSessionID, nil
}

// writeTempPrompt writes the prompt to a temporary file and returns the path.
// The caller is responsible for removing the file after use.
func writeTempPrompt(prompt string) (string, error) {
	dir := tempPromptDir()
	f, err := os.CreateTemp(dir, "graphit-prompt-*.md")
	if err != nil {
		return "", err
	}
	path := f.Name()

	if _, err := f.WriteString(prompt); err != nil {
		f.Close()
		os.Remove(path)
		return "", err
	}
	if err := f.Close(); err != nil {
		os.Remove(path)
		return "", err
	}

	if chmodErr := os.Chmod(path, 0o600); chmodErr != nil {
		// Best effort — non-fatal on systems that don't support chmod
		_ = chmodErr
	}
	return path, nil
}

// tempPromptDir returns the directory for temp prompt files.
// Uses XDG_RUNTIME_DIR if available (RAM-backed, more secure), falls back to os.TempDir.
func tempPromptDir() string {
	if d := os.Getenv("XDG_RUNTIME_DIR"); d != "" {
		subDir := filepath.Join(d, "graphit")
		if err := os.MkdirAll(subDir, 0o700); err == nil {
			return subDir
		}
	}
	return os.TempDir()
}

var ideToCLI = config.CLIForIDE

func tryFallbackCLI(provider string, userCLI string) Client {
	var defaultCandidates []string

	switch provider {
	case "google":
		defaultCandidates = []string{"agy", "gemini", "kiro-cli", "cursor-agent", "agent", "opencode", "copilot"}
	case "anthropic":
		defaultCandidates = []string{"claude", "kiro-cli", "cursor-agent", "agent", "opencode", "copilot"}
	case "openai":
		defaultCandidates = []string{"codex", "cursor-agent", "agent", "opencode", "kiro-cli", "copilot"}
	case "xai":
		defaultCandidates = []string{"grok", "cursor-agent", "agent", "opencode", "kiro-cli", "copilot"}
	case "amazon", "aws":
		defaultCandidates = []string{"kiro-cli", "cursor-agent", "agent", "opencode", "gemini", "copilot"}
	default:

		defaultCandidates = []string{"opencode", "agy", "gemini", "claude", "codex", "grok", "kiro-cli", "cursor-agent", "agent", "copilot", "cline", "goose", "openhands"}
	}

	var candidates []string

	if userCLI != "" {
		candidates = append(candidates, userCLI)
	}

	ide := config.DefaultIDE()
	if equivalent := ideToCLI(ide); equivalent != "" && equivalent != userCLI {
		candidates = append(candidates, equivalent)
	}

	for _, cand := range defaultCandidates {
		if cand != userCLI && cand != ideToCLI(ide) {
			candidates = append(candidates, cand)
		}
	}

	for _, bin := range candidates {
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

// agentArgsFromConfig resolves the extra arguments for agentic runs, most
// specific key first:
//
//	ai.agent_args.<binary>   e.g. ai.agent_args.claude
//	ai.agent_args            applies to whichever CLI is selected
//
// The value is split on whitespace. This is deliberately operator-configured
// rather than a built-in table: the flag that grants workspace write differs per
// CLI, changes between releases, and carries real blast radius — a wrong guess
// either fails to parse or hands the agent more authority than intended.
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
