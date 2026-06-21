package daemon

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

const (
	memorySyncPollInterval = 10 * time.Second
	memorySyncDebounce     = 1 * time.Second
)

type MemorySyncModule struct{}

func NewMemorySyncModule() *MemorySyncModule {
	return &MemorySyncModule{}
}

func (m *MemorySyncModule) Start(ctx context.Context) error {
	g, err := git.DefaultErr()
	if err != nil {
		return fmt.Errorf("memory-sync module requires git: %w", err)
	}
	hashes := make(map[string]string)

	ticker := time.NewTicker(memorySyncPollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			if !m.poll(ctx, g, hashes) {
				return ctx.Err()
			}
		}
	}
}

func (m *MemorySyncModule) poll(ctx context.Context, g git.Git, hashes map[string]string) bool {
	store, err := memory.NewMemoryGitStore()
	if err != nil {
		return true
	}

	branches, err := store.ActiveMemoryBranches()
	if err != nil {
		return true
	}

	repoDir := store.Dir()
	wtBase := repoDir + "-wt"

	for _, branch := range branches {
		wtDir := worktreeDirForBranch(wtBase, branch)
		if _, err := os.Stat(filepath.Join(wtDir, ".git")); err != nil {
			continue
		}

		hash := memoryWorktreeHash(g, wtDir)
		prev, seen := hashes[branch]
		if !seen {
			hashes[branch] = hash
			continue
		}
		if hash == prev {
			continue
		}

		select {
		case <-ctx.Done():
			return false
		case <-time.After(memorySyncDebounce):
		}
		hashes[branch] = memoryWorktreeHash(g, wtDir)

		scope, scopeID := parseBranch(branch)
		wikiDir := memory.MemoryWikiGlobalDir(scope, scopeID)
		memory.RunCycle(ctx, scope, wtDir, wikiDir)
	}
	return true
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

func memoryWorktreeHash(g git.Git, wtDir string) string {
	status, _ := g.RunOutput(wtDir, "status", "--porcelain", "-uall")
	head, _ := g.RunOutput(wtDir, "rev-parse", "HEAD")

	mtimes := dirtyFileMtimes(status, wtDir)
	combined := head + "\n" + status + "\n" + mtimes
	h := sha256.Sum256([]byte(combined))
	return fmt.Sprintf("%x", h[:8])
}
