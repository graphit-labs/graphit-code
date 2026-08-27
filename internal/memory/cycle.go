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
	return runScopeCycle(ctx, "project")
}

func RunUserCycle(ctx context.Context) *CycleResult {
	return runScopeCycle(ctx, "user")
}

// runScopeCycle compiles a scope's raw directory into its wiki.
//
// There is one compile and one destination. Two earlier arrangements are gone: it
// used to compile straight into a project-local replica while the daemon compiled
// into the global wiki — two files, two inodes, and whichever ran last decided what
// a project could recall — and then it compiled globally and copied outward, which
// still meant a copy per reader and a fan-out that could silently fall behind.
func runScopeCycle(ctx context.Context, scope string) *CycleResult {
	scopeID := resolveScopeID(scope)
	if scopeID == "" {
		return &CycleResult{Scope: scope}
	}
	return RunCycle(ctx, scope, RawDirFor(scope, scopeID), MemoryWikiGlobalDir(scope, scopeID))
}

type MemoryStoreProvider interface {
	ExtractScopeDir(scopePath, relDir, targetDir string) error
}

func SyncContextFromMemoryRepo(ctx context.Context, contextName, _ string, provider MemoryStoreProvider, logger *slog.Logger) *CycleResult {
	log := slogutil.Resolve(logger)
	rawDir := RawDirFor(contextName, contextName)
	scopePath := fmt.Sprintf("memory/project/%s", contextName)

	if provider != nil {
		if err := provider.ExtractScopeDir(scopePath, ".", rawDir); err != nil {
			log.Warn("sync context: extract scope failed", "context", contextName, "scope", scopePath, "error", err)
		}
	}

	return RunCycle(ctx, contextName, rawDir, contextWikiDir(contextName))
}

func OnHubImport(ctx context.Context, contextName, projectDir string, store MemoryStoreProvider, logger *slog.Logger) {
	log := slogutil.Resolve(logger)

	go func() {
		if res := SyncContextFromMemoryRepo(ctx, contextName, projectDir, store, logger); res.Err != nil {
			log.Warn("hub import context failed", "context", contextName, "error", res.Err)
		}
	}()
}
