package ai

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type Client interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

// SessionClient extends Client with conversation continuity.
// Consumers can type-assert: if sc, ok := client.(SessionClient); ok { ... }
type SessionClient interface {
	Client
	// CompleteWithSession sends a prompt within an existing session.
	// Pass empty sessionID to start a new session.
	// Returns the response and the session ID for future calls.
	CompleteWithSession(ctx context.Context, sessionID, systemPrompt, userPrompt string) (response string, newSessionID string, err error)
	// SupportsSession reports whether this CLI supports conversation continuity.
	SupportsSession() bool
}

func NewClientFromConfig() (Client, error) {
	cfg, err := config.LoadGlobalConfig()
	if err != nil {
		return nil, err
	}

	cliBinary, _ := config.GetConfigValue(cfg, "ai.cli")
	if cliBinary == "" {
		cliBinary = config.DefaultCLI()
	}

	if cli := tryFallbackCLI("", cliBinary); cli != nil {
		return cli, nil
	}

	return nil, errors.New("AI CLI not found, please install a supported CLI tool (e.g. gemini, claude, codex, qwen, kimi)")
}

// NewClientForAgent binds an explicit agent selection to its own executable.
// Unlike automatic completion discovery, Live Search must not silently execute
// a different CLI from the adapter used to prepare the investigation.
func NewClientForAgent(agent string) (StreamClient, error) {
	agent = strings.ToLower(strings.TrimSpace(agent))
	binary := config.CLIForAgent(agent)
	if binary == "" {
		return nil, fmt.Errorf("agent %q has no supported Live Search CLI", agent)
	}
	path, err := exec.LookPath(binary)
	if err != nil {
		return nil, fmt.Errorf("agent %q requires CLI %q on PATH; install and authenticate that CLI: %w", agent, binary, err)
	}
	return &cliClient{executablePath: path, binaryName: binary, agentArgs: agentArgsFromConfig(binary)}, nil
}
