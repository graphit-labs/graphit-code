package daemon

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
)

const (
	syncDebounce    = 1 * time.Second
	syncMaxDebounce = 5 * time.Second
)

type SyncModule struct {
	projectDir string
	cacheDir   string
	onActivity func()
	lastCommit string
	triggers   ProjectTriggers
}

func NewSyncModule(projectDir, cacheDir string) *SyncModule {
	return &SyncModule{projectDir: projectDir, cacheDir: cacheDir}
}

func (m *SyncModule) Name() string { return "sync" }

// WithTriggers wakes the project's embedders whenever an index this module ran changed.
func (m *SyncModule) WithTriggers(t ProjectTriggers) *SyncModule {
	m.triggers = t
	return m
}

// hubStorageMayExist reports whether a project could have a published Hub base to
// hydrate. It mirrors the early returns of hub.HydrateProjectLanceWithResult that need
// no network — no project identity, or a local provider without Hub storage — so the
// daemon does not start a `hub hydrate` process that has nothing to do. A Broker
// provider learns its storage only over the network, so it always hydrates.
var hubStorageMayExist = func(projectDir string) bool {
	lf, err := hub.LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lf == nil || lf.Project.ID == "" {
		return false
	}
	if snapshot, err := auth.ResolveActive(context.Background()); err == nil && snapshot.Provider.Type == auth.ProviderBroker {
		return true
	}
	return config.HubS3Config().Configured()
}

// SetActivityCallback implements ActivityReporter. cb is invoked whenever a
// batch of filesystem events arrives, even if nothing in it is reindexable —
// any change under the watched tree counts as the project being worked on.
func (m *SyncModule) SetActivityCallback(cb func()) { m.onActivity = cb }

func (m *SyncModule) Start(ctx context.Context) error {
	astIgnore := ast.NewAstIgnoreChecker(m.projectDir)
	wikiIgnore := knowledge.NewKnowledgeIgnoreChecker(m.projectDir)

	w, err := fswatch.New(fswatch.Config{
		Root:        m.projectDir,
		Ignore:      ignoreUnion{astIgnore, wikiIgnore},
		Debounce:    syncDebounce,
		MaxDebounce: syncMaxDebounce,
	})
	if err != nil {
		return fmt.Errorf("sync module: start watcher: %w", err)
	}
	defer func() { _ = w.Close() }()

	batches, err := w.Start(ctx)
	if err != nil {
		return fmt.Errorf("sync module: watch %s: %w", m.projectDir, err)
	}
	// A parked supervisor has no watcher. Reconcile on every start so changes
	// (including deletions) made while it was parked cannot remain in the indexes.
	slog.Info("daemon: reconciling indexes on watcher start", "project", m.projectDir)
	m.handleBatch(ctx, fswatch.Batch{Rescan: true}, astIgnore, wikiIgnore)

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case batch, ok := <-batches:
			if !ok {
				return ctx.Err()
			}
			if m.onActivity != nil {
				m.onActivity()
			}
			m.handleBatch(ctx, batch, astIgnore, wikiIgnore)
		}
	}
}

type ignoreUnion []fswatch.Ignorer

func (u ignoreUnion) IsIgnored(relPath string, isDir bool) bool {
	for _, ig := range u {
		if ig == nil {
			continue
		}
		if !ig.IsIgnored(relPath, isDir) {
			return false
		}
	}
	return len(u) > 0
}

func (u ignoreUnion) ShouldDescend(dirRelPath string) bool {
	for _, ig := range u {
		if ig != nil && ig.ShouldDescend(dirRelPath) {
			return true
		}
	}
	return false
}

// At deepens each member into the directory, keeping the union's semantics: a
// path is skipped only when EVERY member says to skip it, with each member
// carrying now the ignore files of the directories it crossed.
func (u ignoreUnion) At(dirRelPath string) fswatch.Ignorer {
	next := make(ignoreUnion, 0, len(u))
	for _, ig := range u {
		if ig != nil {
			next = append(next, ig.At(dirRelPath))
		}
	}
	return next
}

type batchTargets struct {
	astChanged []string
	astRemoved []string
	knowledge  bool
}

func classifyBatch(batch fswatch.Batch, projectDir, docsPath string, knowledgeExts map[string]bool,
	astIgnore, wikiIgnore fswatch.Ignorer, extraDocs []string) batchTargets {
	var t batchTargets

	isExtraDoc := func(slashRel string) bool {
		for _, extra := range extraDocs {
			if filepath.ToSlash(extra) == slashRel {
				return true
			}
		}
		return false
	}

	classify := func(paths []string, removed bool) {
		for _, p := range paths {
			rel, err := filepath.Rel(projectDir, p)
			if err != nil || insideDotDir(rel) {
				continue
			}
			ext := strings.ToLower(filepath.Ext(p))
			slashRel := filepath.ToSlash(rel)

			if knowledgeExts[ext] && (isUnder(p, docsPath) || isExtraDoc(slashRel)) &&
				!ignores(wikiIgnore, slashRel) {
				t.knowledge = true
			}

			if ignores(astIgnore, slashRel) {
				continue
			}
			if !ast.HasParserForExtensionIn(projectDir, ext) {
				continue
			}
			if removed {
				t.astRemoved = append(t.astRemoved, filepath.ToSlash(rel))
				continue
			}
			t.astChanged = append(t.astChanged, filepath.ToSlash(rel))
		}
	}
	classify(batch.Changed, false)
	classify(batch.Removed, true)

	return t
}

// hydrationReport mirrors the JSON `graphit hub hydrate --json` prints.
type hydrationReport struct {
	ASTBaseChanged       bool `json:"ast_base_changed"`
	KnowledgeBaseChanged bool `json:"knowledge_base_changed"`
}

// handleBatch turns one batch of filesystem events into CLI children scoped to the
// project. The watcher start (Rescan) runs a full `graphit sync`; a batch refreshes the
// published base with `hub hydrate --json`, then indexes exactly what changed with
// `ast index --changes-from -` and `knowledge index`. The children take the machine-wide
// heavy-work slot themselves; the daemon only decides what to run.
func (m *SyncModule) handleBatch(ctx context.Context, batch fswatch.Batch,
	astIgnore, wikiIgnore fswatch.Ignorer) {
	if ctx.Err() != nil {
		return
	}
	projectCfg := loadProjectConfigFromDir(m.projectDir)

	scope := knowledge.ScopeFor(m.projectDir, nil, projectCfg)
	docsPath := filepath.Join(m.projectDir, scope.Subdir)
	targets := classifyBatch(batch, m.projectDir, docsPath,
		config.ResolveKnowledgeExtensions(nil, projectCfg), astIgnore, wikiIgnore, scope.ExtraFiles)

	astEnabled := !config.IsModuleDisabled("ast", nil, projectCfg)
	knowledgeEnabled := !config.IsModuleDisabled("knowledge", nil, projectCfg)
	astWork := astEnabled && (batch.Rescan || len(targets.astChanged) > 0 || len(targets.astRemoved) > 0)
	knowledgeWork := knowledgeEnabled && (targets.knowledge || batch.Rescan)
	gitHead := ""
	if git, err := gitstate.DefaultErr(); err == nil {
		gitHead, _ = git.RunOutput(m.projectDir, "rev-parse", "HEAD")
	}
	if !astWork && !knowledgeWork && (gitHead == "" || gitHead == m.lastCommit) {
		return
	}

	if batch.Rescan {
		// A parked supervisor has no watcher, so anything — deletions included — may
		// have changed while it was parked: reconcile the whole project.
		slog.Info("daemon: running full project sync", "project", m.projectDir)
		m.lastCommit = gitHead
		if err := runProjectChild(ctx, m.projectDir, nil, "sync"); err != nil {
			if ctx.Err() == nil {
				slog.Error("daemon: project sync failed", "project", m.projectDir, "error", err)
			}
			return
		}
		m.triggers.ASTIndexed.Notify()
		m.triggers.KnowledgeIndexed.Notify()
		return
	}

	var hydrated hydrationReport
	if hubStorageMayExist(m.projectDir) {
		out, err := outputProjectChild(ctx, m.projectDir, "hub", "hydrate", "--json")
		if err != nil {
			if ctx.Err() == nil {
				slog.Error("daemon: published Lance hydration failed", "project", m.projectDir, "error", err)
			}
			return
		}
		if err := json.Unmarshal(out, &hydrated); err != nil {
			slog.Error("daemon: unreadable hydration report", "project", m.projectDir, "error", err)
			return
		}
	}
	m.lastCommit = gitHead
	fullAST := false
	if hydrated.ASTBaseChanged && astEnabled {
		astWork, fullAST = true, true
	}
	if hydrated.KnowledgeBaseChanged && knowledgeEnabled {
		knowledgeWork = true
	}

	var err error
	if astWork {
		if fullAST {
			slog.Info("daemon: published AST base changed; reconciling the whole project", "project", m.projectDir)
			err = runProjectChild(ctx, m.projectDir, nil, "ast", "index")
		} else {
			changes, _ := json.Marshal(map[string][]string{"changed": targets.astChanged, "removed": targets.astRemoved})
			err = runProjectChild(ctx, m.projectDir, bytes.NewReader(changes), "ast", "index", "--changes-from", "-")
		}
		switch {
		case err == nil:
			m.triggers.ASTIndexed.Notify()
		case ctx.Err() == nil:
			slog.Error("daemon: AST index failed", "project", m.projectDir, "error", err)
		}
	}
	if knowledgeWork && ctx.Err() == nil {
		switch err := runProjectChild(ctx, m.projectDir, nil, "knowledge", "index"); {
		case err == nil:
			m.triggers.KnowledgeIndexed.Notify()
		case ctx.Err() == nil:
			slog.Error("daemon: Knowledge index failed", "project", m.projectDir, "error", err)
		}
	}
}

func ignores(ig fswatch.Ignorer, relPath string) bool {
	return ig != nil && ig.IsIgnored(relPath, false)
}

func insideDotDir(rel string) bool {
	dir := filepath.ToSlash(filepath.Dir(rel))
	if dir == "." || dir == "" {
		return false
	}
	for _, seg := range strings.Split(dir, "/") {
		if strings.HasPrefix(seg, ".") {
			return true
		}
	}
	return false
}

func isUnder(path, dir string) bool {
	rel, err := filepath.Rel(dir, path)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}
