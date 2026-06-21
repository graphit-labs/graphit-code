package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type CycleResult struct {
	Scope     string
	WikiFiles int
	Err       error
}

func RunCycle(ctx context.Context, scope, rawDir, wikiDir string) *CycleResult {
	res := &CycleResult{Scope: scope}

	if _, err := os.Stat(rawDir); os.IsNotExist(err) {

		return res
	}

	wikiRes, err := GenerateMemoryWiki(ctx, rawDir, wikiDir)
	if err != nil {
		res.Err = fmt.Errorf("memory wiki (%s): %w", scope, err)
		return res
	}
	res.WikiFiles = wikiRes.ArticlesWritten
	return res
}

func RunProjectCycle(ctx context.Context) *CycleResult {
	rawDir := RawDir("project")
	return RunCycle(ctx, "project", rawDir, WikiDir("project"))
}

func RunUserCycle(ctx context.Context) *CycleResult {
	rawDir := RawDir("user")
	return RunCycle(ctx, "user", rawDir, WikiDir("user"))
}

type MemoryStoreProvider interface {
	ExtractBranchDir(branch, relDir, targetDir string) error
}

func SyncContextFromMemoryRepo(ctx context.Context, contextName, projectDir string, store MemoryStoreProvider, logger *slog.Logger) *CycleResult {
	log := slogutil.Resolve(logger)
	EnsureContextCopy(contextName, projectDir, logger)
	rawDir := RawDir(contextName)
	branch := fmt.Sprintf("memory/project/%s", contextName)

	if store != nil {
		if err := store.ExtractBranchDir(branch, ".", rawDir); err != nil {
			log.Warn("sync context: extract branch failed", "context", contextName, "branch", branch, "error", err)
		}
	}

	return RunCycle(ctx, contextName, rawDir, WikiDir(contextName))
}

func OnHubImport(ctx context.Context, contextName, projectDir string, store MemoryStoreProvider, logger *slog.Logger) {
	log := slogutil.Resolve(logger)

	go func() {
		if res := SyncContextFromMemoryRepo(ctx, contextName, projectDir, store, logger); res.Err != nil {
			log.Warn("hub import context failed", "context", contextName, "error", res.Err)
		}
	}()
}
