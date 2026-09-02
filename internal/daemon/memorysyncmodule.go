package daemon

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

const (
	memorySyncDebounce    = 1 * time.Second
	memorySyncMaxDebounce = 10 * time.Second
)

type MemorySyncModule struct {
	Logger *slog.Logger
}

func NewMemorySyncModule() *MemorySyncModule {
	return &MemorySyncModule{}
}

// Name is what the supervisor logs this module as.
//
// It covers BOTH memory scopes — a project's own and the user's — because every scope's raw
// directory lives under the one root this watches recursively. See Start.
func (m *MemorySyncModule) Name() string { return "memory-sync" }

func (m *MemorySyncModule) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

// Start recompiles a memory wiki whenever its raw directory changes.
//
// Every scope's raw directory lives under a single root that this tool owns, so one recursive
// watch covers all of them — including scopes created later, which the watcher picks up when
// their directory appears. This replaced a 10s poll that ran `git status` once per active scope
// per tick.
//
// THE ROOT IS store.Dir() AND NOTHING APPENDED TO IT. It used to be `store.Dir() + "-wt"`,
// because the store's Dir() was the git repository and the worktrees sat beside it. Dir() is the
// raw root itself now, so the suffix pointed the watcher at an empty directory it created on the
// way — a watch that never fires, and a memory wiki that never recompiles, with nothing to see
// but a stray `memory-raw-wt` in the global directory.
func (m *MemorySyncModule) Start(ctx context.Context) error {
	store, err := memory.NewMemoryStore()
	if err != nil {
		return fmt.Errorf("memory-sync module: open memory store: %w", err)
	}
	rawRoot := store.Dir()
	if err := os.MkdirAll(rawRoot, 0o755); err != nil {
		return fmt.Errorf("memory-sync module: raw memory root %s: %w", rawRoot, err)
	}

	w, err := fswatch.New(fswatch.Config{
		Root:        rawRoot,
		Debounce:    memorySyncDebounce,
		MaxDebounce: memorySyncMaxDebounce,
	})
	if err != nil {
		return fmt.Errorf("memory-sync module: start watcher: %w", err)
	}
	defer func() { _ = w.Close() }()

	batches, err := w.Start(ctx)
	if err != nil {
		return fmt.Errorf("memory-sync module: watch %s: %w", rawRoot, err)
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case batch, ok := <-batches:
			if !ok {
				return ctx.Err()
			}
			m.recompile(ctx, rawRoot, batch)
		}
	}
}

// recompile rebuilds the wiki of every memory scope the batch touched.
func (m *MemorySyncModule) recompile(ctx context.Context, rawRoot string, batch fswatch.Batch) {
	store, err := memory.NewMemoryStore()
	if err != nil {
		return
	}
	scopes, err := store.ActiveScopes()
	if err != nil {
		return
	}

	touched := append(append([]string{}, batch.Changed...), batch.Removed...)

	for _, scopePath := range scopes {
		rawDir := scopeDir(rawRoot, scopePath)
		// 🔒 THERE WAS A `.git` CHECK HERE, AND IT SKIPPED EVERY SCOPE.
		//
		// It read `os.Stat(<rawDir>/.git)` and `continue`d on failure — a liveness test from when a
		// memory scope was a git worktree. Memory left git behind, so nothing creates `.git` any
		// more, and the gate became a condition that is false for every scope: measured on this
		// machine at the time, 4 of 5 raw
		// directories have none, including this project's own. The daemon has therefore not
		// recompiled a memory wiki since git was removed, and it failed as a NO-OP — no error, no
		// log line, just a watcher that noticed every write and did nothing with it.
		//
		// Nothing replaces it because nothing needs to: `memory.RunCycle` already stats rawDir and
		// returns an empty result when it is absent, so the existence check lives where the work is
		// rather than in a gate that has to be kept in step with how a scope is created.
		//
		// A lost-events batch has no reliable path list, so every scope is
		// recompiled; otherwise only the ones with a changed file under them.
		if !batch.Rescan && !anyUnder(touched, rawDir) {
			continue
		}
		scope, scopeID := parseScopePath(scopePath)
		wikiDir := memory.MemoryWikiGlobalDir(scope, scopeID)
		if res := memory.RunCycle(ctx, scope, memory.TableURIFor(scope, scopeID), wikiDir); res.Err != nil {
			m.log().Warn("memory wiki compile failed", "scope", scope, "scope_id", scopeID, "error", res.Err)
			continue
		}
	}
}

// Compiling is the whole job. There used to be a fan-out step here that copied each
// freshly compiled wiki into every project that read it, with a policy per scope —
// project to its owner, user to every registered project, an imported context only
// where a replica already existed. It existed because the wiki a project opened was
// a copy, so a memory arriving from the remote reached nobody until something pushed
// it outward. The wiki is now read where it is compiled, so the push has nothing left
// to do and the failure mode it carried — a project quietly serving a copy nobody
// refreshed — cannot happen.

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

func scopeDir(rawRoot, scopePath string) string {
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(scopePath)
	return filepath.Join(rawRoot, safe)
}

func parseScopePath(scopePath string) (scope, scopeID string) {
	parts := strings.SplitN(scopePath, "/", 3)
	if len(parts) < 3 {
		return "project", ""
	}
	return parts[1], parts[2]
}
