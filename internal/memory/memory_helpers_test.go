package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func EnsureWikiIndexExists(scope string, logger *slog.Logger) {
	log := slogutil.Resolve(logger)
	wikiDir := WikiDir(scope)
	if wikiDir == "" {
		return
	}
	indexPath := filepath.Join(wikiDir, "index.md")
	if _, err := os.Stat(indexPath); err == nil {
		return
	}
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		log.Warn("ensure wiki index: mkdir failed", "dir", wikiDir, "error", err)
	}
	content := fmt.Sprintf("---\ntitle: Memory Wiki (%s)\ntags: [memory, %s]\n---\n\n# Memory Wiki\n\n*(No memories indexed yet. Run `memory index` to populate.)*\n", scope, scope)
	if err := os.WriteFile(indexPath, []byte(content), 0o644); err != nil {
		log.Warn("ensure wiki index: write failed", "path", indexPath, "error", err)
	}
}

func firstLineFromContent(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed != "" && !strings.HasPrefix(trimmed, "#") {
			if len(trimmed) > 100 {
				return trimmed[:100] + "…"
			}
			return trimmed
		}
	}
	return ""
}

func memoryBranch(scope, scopeID string) string {
	if scope == "project" || scope == "user" {
		return fmt.Sprintf("memory/%s/%s", scope, scopeID)
	}
	return fmt.Sprintf("memory/project/%s", scope)
}

func SyncAndCycle(ctx context.Context, scope, scopeID string, store MemoryStoreProvider, logger *slog.Logger) *CycleResult {
	log := slogutil.Resolve(logger)
	rawDir := RawDirFor(scope, scopeID)
	branch := memoryBranch(scope, scopeID)

	if store != nil {
		if err := store.ExtractScopeDir(branch, ".", rawDir); err != nil {
			log.Warn("sync: extract branch failed", "scope", scope, "branch", branch, "error", err)
		}
	}

	return RunCycle(ctx, scope, rawDir, MemoryWikiGlobalDir(scope, scopeID))
}

// memoryEntityPageWithHash renders the page of a LIVE memory.
//
// It is the old signature of memoryEntityPage, kept for the tests that predate the revision
// chain. Production code passes a memDoc, because a page now also has to be able to describe a
// superseded revision — which needs the chain fields this helper deliberately leaves empty.
func memoryEntityPageWithHash(id, title, createdAt string, important bool, body, memType, contentHash string) string {
	return memoryEntityPage(memDoc{
		id:          id,
		title:       title,
		createdAt:   createdAt,
		important:   important,
		body:        body,
		memType:     memType,
		contentHash: contentHash,
		filename:    MemoryFileName(id),
	})
}
