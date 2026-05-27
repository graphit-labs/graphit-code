package ai

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTryFallbackCLIAndComplete(t *testing.T) {
	// Create temp directory for dummy binaries
	tempDir, err := os.MkdirTemp("", "graphit-ai-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	// Create dummy "grok" shell script binary
	dummyGrok := filepath.Join(tempDir, "grok")
	grokScript := `#!/bin/sh
# Echo input and mock response
echo "grok completed"
`
	err = os.WriteFile(dummyGrok, []byte(grokScript), 0755)
	if err != nil {
		t.Fatalf("failed to write dummy grok: %v", err)
	}

	// Set PATH to include tempDir
	origPath := os.Getenv("PATH")
	_ = os.Setenv("PATH", tempDir+":"+origPath)
	defer func() { _ = os.Setenv("PATH", origPath) }()

	// Try finding grok CLI client
	client := tryFallbackCLI("xai", "grok")
	if client == nil {
		t.Fatalf("expected non-nil Grok client")
	}

	cc, ok := client.(*cliClient)
	if !ok {
		t.Fatalf("expected client to be of type *cliClient")
	}
	if cc.binaryName != "grok" {
		t.Errorf("expected grok binary, got %s", cc.binaryName)
	}

	// Run Complete
	ctx := context.Background()
	resp, err := client.Complete(ctx, "System prompt", "User prompt")
	if err != nil {
		t.Errorf("Complete failed: %v", err)
	}
	if strings.TrimSpace(resp) != "grok completed" {
		t.Errorf("expected 'grok completed', got %q", resp)
	}

	// Test fallback path prioritizing a general provider candidate
	clientGeneral := tryFallbackCLI("unknown_provider", "")
	if clientGeneral == nil {
		t.Fatalf("expected client for unknown_provider fallback, since grok is in PATH")
	}
}

func TestNewClientFromConfigError(t *testing.T) {
	// Temporarily clear PATH so no AI binary is found
	origPath := os.Getenv("PATH")
	_ = os.Setenv("PATH", "")
	defer func() { _ = os.Setenv("PATH", origPath) }()

	// Test NewClientFromConfig when no executable is in PATH
	_, err := NewClientFromConfig()
	if err == nil {
		t.Error("expected error from NewClientFromConfig when PATH is empty")
	}
}
