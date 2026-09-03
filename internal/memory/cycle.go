package memory

import (
	"context"
	"fmt"
	"log/slog"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type CycleResult struct {
	Scope     string
	WikiFiles int
	Err       error
}

func RunCycle(ctx context.Context, scope, tableURI, wikiDir string) *CycleResult {
	res := &CycleResult{Scope: scope}

	if tableURI == "" {
		return res
	}

	tbl, err := OpenMemoryTable(ctx, tableURI)
	if err != nil {
		res.Err = fmt.Errorf("opening the memory store (%s): %w", scope, err)
		return res
	}
	defer func() { _ = tbl.Close() }()

	wikiRes, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir)
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

func runScopeCycle(ctx context.Context, scope string) *CycleResult {
	scopeID := resolveScopeID(scope)
	if scopeID == "" {
		return &CycleResult{Scope: scope}
	}
	return RunCycle(ctx, scope, TableURIFor(scope, scopeID), MemoryWikiGlobalDir(scope, scopeID))
}

func SyncContextFromMemoryRepo(ctx context.Context, contextName string) *CycleResult {
	return RunCycle(ctx, contextName, ContextTableURI(contextName), contextWikiDir(contextName))
}

// OnHubImport compiles a freshly imported context's memory wiki, in the background.
//
// It took a project directory and a store provider, and needs neither now: a context's memories are
// a table at a prefix derived from its NAME, so there is nothing to locate per project and nothing
// to download before reading.
func OnHubImport(ctx context.Context, contextName string, logger *slog.Logger) <-chan struct{} {
	log := slogutil.Resolve(logger)
	done := make(chan struct{})
	go func() {
		defer close(done)
		if res := SyncContextFromMemoryRepo(ctx, contextName); res.Err != nil {
			log.Warn("hub import context failed", "context", contextName, "error", res.Err)
		}
	}()
	return done
}
