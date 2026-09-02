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

	// There is no existence check ahead of this. The raw directory had one — a scope with no
	// directory had nothing to compile — but opening a table CREATES it, so the equivalent question
	// is answered by the row count, and an empty scope compiles to an empty wiki in one query.
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
	return RunCycle(ctx, scope, TableURIFor(scope, scopeID), MemoryWikiGlobalDir(scope, scopeID))
}

// SyncContextFromMemoryRepo compiles an imported context's wiki from that context's table.
//
// It used to DOWNLOAD the context first: a `MemoryStoreProvider` materialised the remote prefix into
// a local raw directory and the compile then read those files. Both the interface and the download
// are gone — the table at `memory/project/<name>` is read where it lives, which is the point of the
// store being in object storage.
func SyncContextFromMemoryRepo(ctx context.Context, contextName string) *CycleResult {
	return RunCycle(ctx, contextName, ContextTableURI(contextName), contextWikiDir(contextName))
}

// OnHubImport compiles a freshly imported context's memory wiki, in the background.
//
// It took a project directory and a store provider, and needs neither now: a context's memories are
// a table at a prefix derived from its NAME, so there is nothing to locate per project and nothing
// to download before reading.
func OnHubImport(ctx context.Context, contextName string, logger *slog.Logger) {
	log := slogutil.Resolve(logger)
	go func() {
		if res := SyncContextFromMemoryRepo(ctx, contextName); res.Err != nil {
			log.Warn("hub import context failed", "context", contextName, "error", res.Err)
		}
	}()
}
