package mcpstdio

import (
	"context"
	"errors"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestProjectVersionToolCancelsWaitingForDaemonJob(t *testing.T) {
	dir := t.TempDir()
	release, err := projectversion.Acquire(dir, "job")
	if err != nil {
		t.Fatal(err)
	}
	defer release()
	type input struct{ ProjectDir, ProjectVersion string }
	handler := withProjectVersion(func(context.Context, *mcp.CallToolRequest, input) (*mcp.CallToolResult, int, error) {
		t.Error("canceled handler ran")
		return nil, 0, nil
	})
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, _, err = handler(ctx, nil, input{ProjectDir: dir})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("wait: %v", err)
	}
}
