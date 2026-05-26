package ai

import (
	"context"
	"errors"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type Client interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
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

	return nil, errors.New("AI CLI not found. Please install a supported CLI tool (e.g., gemini, claude, codex, cursor-agent).")
}
