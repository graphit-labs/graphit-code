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

const argMaxSafe = 128 * 1024

func (c *cliClient) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	var promptBuilder strings.Builder
	if systemPrompt != "" {
		promptBuilder.WriteString(systemPrompt)
		promptBuilder.WriteString("\n\n")
	}
	promptBuilder.WriteString(userPrompt)

	prompt := promptBuilder.String()
	useStdin := len(prompt) > argMaxSafe

	var args []string
	switch c.binaryName {
	case "claude", "gemini", "agy", "grok":

		if useStdin {
			args = []string{"-p", "-"}
		} else {
			args = []string{"-p", prompt}
		}
	case "cursor-agent", "agent":
		if useStdin {
			args = []string{"-p", "-"}
		} else {
			args = []string{"-p", prompt}
		}
	case "codex":
		if useStdin {
			args = []string{"run", "-"}
		} else {
			args = []string{"run", prompt}
		}
	case "opencode":
		if useStdin {
			args = []string{"run", "-"}
		} else {
			args = []string{"run", prompt}
		}
	case "kiro-cli":
		if useStdin {
			args = []string{"chat", "--no-interactive", "-"}
		} else {
			args = []string{"chat", "--no-interactive", prompt}
		}
	default:
		if useStdin {
			args = []string{"-"}
		} else {
			args = []string{prompt}
		}
	}

	cmd := exec.CommandContext(ctx, c.executablePath, args...)

	var outBuf, errBuf bytes.Buffer
	cmd.Stdout = &outBuf
	cmd.Stderr = &errBuf

	if useStdin {
		cmd.Stdin = strings.NewReader(prompt)
	}

	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("CLI fallback %q failed: %w (stderr: %s)", c.binaryName, err, errBuf.String())
	}

	return strings.TrimSpace(outBuf.String()), nil
}

var ideToCLI = config.CLIForIDE

func tryFallbackCLI(provider string, userCLI string) Client {
	var defaultCandidates []string

	switch provider {
	case "google":
		defaultCandidates = []string{"agy", "gemini", "kiro-cli", "cursor-agent", "agent", "opencode"}
	case "anthropic":
		defaultCandidates = []string{"claude", "kiro-cli", "cursor-agent", "agent", "opencode"}
	case "openai":
		defaultCandidates = []string{"codex", "cursor-agent", "agent", "opencode", "kiro-cli"}
	case "xai":
		defaultCandidates = []string{"grok", "cursor-agent", "agent", "opencode", "kiro-cli"}
	case "amazon", "aws":
		defaultCandidates = []string{"kiro-cli", "cursor-agent", "agent", "opencode", "gemini"}
	default:

		defaultCandidates = []string{"opencode", "agy", "gemini", "claude", "codex", "grok", "kiro-cli", "cursor-agent", "agent"}
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
