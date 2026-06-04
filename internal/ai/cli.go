package ai

import (
	"bytes"
	"context"
	"fmt"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type cliClient struct {
	executablePath string
	binaryName     string
}

// cliSpec defines how to invoke a specific CLI binary.
type cliSpec struct {
	// stdinArgs are the arguments used when passing prompt via stdin (always).
	stdinArgs []string
	// sessionFlag is the flag name for resuming a conversation (e.g. "--conversation").
	// Empty means the CLI does not support session continuity.
	sessionFlag string
}

// knownCLIs maps binary names to their invocation spec.
// All CLIs receive prompts via stdin to avoid exposing text in ps.
var knownCLIs = map[string]cliSpec{
	"claude":       {stdinArgs: []string{"-p", "-"}, sessionFlag: "--resume"},
	"gemini":       {stdinArgs: []string{"-p", "-"}, sessionFlag: "--resume"},
	"agy":          {stdinArgs: []string{"-p", "-"}, sessionFlag: "--conversation"},
	"grok":         {stdinArgs: []string{"-p", "-"}},
	"cursor-agent": {stdinArgs: []string{"-p", "-"}},
	"agent":        {stdinArgs: []string{"-p", "-"}},
	"codex":        {stdinArgs: []string{"exec", "-"}},
	"opencode":     {stdinArgs: []string{"run", "-"}, sessionFlag: "-s"},
	"kiro-cli":     {stdinArgs: []string{"chat", "--no-interactive", "-"}},
	"copilot":      {stdinArgs: []string{"-p", "-"}, sessionFlag: "--resume"},
	"cline":        {stdinArgs: []string{"-p", "-"}},
	"goose":        {stdinArgs: []string{"run", "-t", "-"}},
	"openhands":    {stdinArgs: []string{"--headless", "-t", "-"}},
}

func specForBinary(name string) cliSpec {
	if spec, ok := knownCLIs[name]; ok {
		return spec
	}
	return cliSpec{stdinArgs: []string{"-"}}
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
	if systemPrompt != "" {
		promptBuilder.WriteString(systemPrompt)
		promptBuilder.WriteString("\n\n")
	}
	promptBuilder.WriteString(userPrompt)

	prompt := promptBuilder.String()
	spec := specForBinary(c.binaryName)

	args := make([]string, len(spec.stdinArgs))
	copy(args, spec.stdinArgs)

	// Prepend session flag if session ID is provided and CLI supports it
	if sessionID != "" && spec.sessionFlag != "" {
		args = append([]string{spec.sessionFlag, sessionID}, args...)
	}

	cmd := exec.CommandContext(ctx, c.executablePath, args...)

	var outBuf, errBuf bytes.Buffer
	cmd.Stdout = &outBuf
	cmd.Stderr = &errBuf
	cmd.Stdin = strings.NewReader(prompt)

	if err := cmd.Run(); err != nil {
		return "", "", fmt.Errorf("CLI fallback %q failed: %w (stderr: %s)", c.binaryName, err, errBuf.String())
	}

	response := strings.TrimSpace(outBuf.String())

	// For session-capable CLIs, return the same sessionID back.
	// The external CLI manages the actual session state.
	returnedSessionID := sessionID
	if returnedSessionID == "" && spec.sessionFlag != "" {
		// First call: we don't have a session ID from the CLI output yet.
		// Session continuity requires the caller to supply an ID externally
		// (e.g. from a previous interactive session).
		returnedSessionID = ""
	}

	return response, returnedSessionID, nil
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
			}
		}
	}

	return nil
}
