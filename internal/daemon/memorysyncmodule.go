package daemon

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

const (
	memorySyncDebounce    = 1 * time.Second
	memorySyncMaxDebounce = 10 * time.Second
)

type MemorySyncModule struct{}

func NewMemorySyncModule() *MemorySyncModule {
	return &MemorySyncModule{}
}

// Start recompiles a memory wiki whenever its worktree changes.
//
// The worktrees live under a single base directory that this tool owns, so one
// recursive watch covers every branch — including worktrees created later, which
// the watcher picks up when their directory appears. This replaces a 10s git
// poll that ran `git status` once per active branch per tick.
func (m *MemorySyncModule) Start(ctx context.Context) error {
	store, err := memory.NewMemoryGitStore()
	if err != nil {
		return fmt.Errorf("memory-sync module: open memory store: %w", err)
	}
	wtBase := store.Dir() + "-wt"
	if err := os.MkdirAll(wtBase, 0o755); err != nil {
		return fmt.Errorf("memory-sync module: worktree base %s: %w", wtBase, err)
	}

	w, err := fswatch.New(fswatch.Config{
		Root:        wtBase,
		Debounce:    memorySyncDebounce,
		MaxDebounce: memorySyncMaxDebounce,
	})
	if err != nil {
		return fmt.Errorf("memory-sync module: start watcher: %w", err)
	}
	defer func() { _ = w.Close() }()

	batches, err := w.Start(ctx)
	if err != nil {
		return fmt.Errorf("memory-sync module: watch %s: %w", wtBase, err)
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case batch, ok := <-batches:
			if !ok {
				return ctx.Err()
			}
			m.recompile(ctx, wtBase, batch)
		}
	}
}

// recompile rebuilds the wiki of every memory branch the batch touched.
func (m *MemorySyncModule) recompile(ctx context.Context, wtBase string, batch fswatch.Batch) {
	store, err := memory.NewMemoryGitStore()
	if err != nil {
		return
	}
	branches, err := store.ActiveMemoryBranches()
	if err != nil {
		return
	}

	touched := append(append([]string{}, batch.Changed...), batch.Removed...)

	for _, branch := range branches {
		wtDir := worktreeDirForBranch(wtBase, branch)
		if _, err := os.Stat(filepath.Join(wtDir, ".git")); err != nil {
			continue
		}
		// A lost-events batch has no reliable path list, so every branch is
		// recompiled; otherwise only the ones with a changed file under them.
		if !batch.Rescan && !anyUnder(touched, wtDir) {
			continue
		}
		scope, scopeID := parseBranch(branch)
		wikiDir := memory.MemoryWikiGlobalDir(scope, scopeID)
		memory.RunCycle(ctx, scope, wtDir, wikiDir)
	}
}

// anyUnder reports whether any path lies inside dir.
func anyUnder(paths []string, dir string) bool {
	for _, p := range paths {
		rel, err := filepath.Rel(dir, p)
		if err != nil {
			continue
		}
		if rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

func worktreeDirForBranch(wtBase, branch string) string {
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(branch)
	return filepath.Join(wtBase, safe)
}

func parseBranch(branch string) (scope, scopeID string) {
	parts := strings.SplitN(branch, "/", 3)
	if len(parts) < 3 {
		return "project", ""
	}
	return parts[1], parts[2]
}

// dirtyFileMtimes renders "path:mtime" lines for the dirty files in a git
// porcelain listing. The memory module still polls its worktrees in ~/.graphit
// (they are written by the MCP layer, not by an editor, so notification latency
// does not matter there); project trees are watched instead — see fswatch.
