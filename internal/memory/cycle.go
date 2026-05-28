package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
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

func RunContextCycle(ctx context.Context, contextName string) *CycleResult {
	rawDir := RawDir(contextName)
	return RunCycle(ctx, contextName,
		rawDir,
		WikiDir(contextName),
	)
}

func RunAllContextCycles(ctx context.Context) []*CycleResult {
	var results []*CycleResult
	for _, name := range AllContextDirs() {
		results = append(results, RunContextCycle(ctx, name))
	}
	return results
}

func SyncAndCycle(ctx context.Context, scope, scopeID string, store MemoryStoreProvider) *CycleResult {
	rawDir := WorktreeRawDir(scope, scopeID)
	branch := memoryBranch(scope, scopeID)

	if store != nil {

		if err := store.ExtractBranchDir(branch, ".", rawDir); err != nil {

			fmt.Fprintf(os.Stderr, "[memory] sync %s: extract branch %q failed: %v\n", scope, branch, err)
		}
	}

	return RunCycle(ctx, scope, rawDir, WikiDir(scope))
}

type MemoryStoreProvider interface {
	ExtractBranchDir(branch, relDir, targetDir string) error
}

func SyncContextFromMemoryRepo(ctx context.Context, contextName, projectDir string, store MemoryStoreProvider) *CycleResult {
	EnsureContextCopy(contextName, projectDir)
	rawDir := RawDir(contextName)
	branch := fmt.Sprintf("memory/project/%s", contextName)

	if store != nil {
		if err := store.ExtractBranchDir(branch, ".", rawDir); err != nil {
			fmt.Fprintf(os.Stderr, "[memory] sync context %q: extract branch %q failed: %v\n", contextName, branch, err)
		}
	}

	return RunCycle(ctx, contextName, rawDir, WikiDir(contextName))
}

func OnHubImport(ctx context.Context, contextName, projectDir string, store MemoryStoreProvider) {

	go func() {
		if res := SyncContextFromMemoryRepo(ctx, contextName, projectDir, store); res.Err != nil {
			fmt.Fprintf(os.Stderr, "[memory] hub import context %q: %v\n", contextName, res.Err)
		}
	}()
}

func memoryBranch(scope, scopeID string) string {
	if scope == "project" || scope == "user" {
		return fmt.Sprintf("memory/%s/%s", scope, scopeID)
	}
	return fmt.Sprintf("memory/project/%s", scope)
}

func EnsureWikiIndexExists(scope string) {
	wikiDir := WikiDir(scope)
	indexPath := filepath.Join(wikiDir, "index.md")
	if _, err := os.Stat(indexPath); err == nil {
		return
	}
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] ensure wiki index: mkdir %s failed: %v\n", wikiDir, err)
	}
	content := fmt.Sprintf("---\ntitle: Memory Wiki (%s)\ntags: [memory, %s]\n---\n\n# Memory Wiki\n\n*(No memories indexed yet. Run `memory index` to populate.)*\n", scope, scope)
	if err := os.WriteFile(indexPath, []byte(content), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] ensure wiki index: write %s failed: %v\n", indexPath, err)
	}
}
