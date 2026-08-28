package daemon

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

const (
	// syncDebounce is the quiet period before a burst of filesystem events is
	// turned into one reindex; syncMaxDebounce caps how long a continuously
	// busy tree may defer that reindex.
	syncDebounce    = 1 * time.Second
	syncMaxDebounce = 5 * time.Second
)

type SyncModule struct {
	projectDir string
	cacheDir   string
	onActivity func()
}

func NewSyncModule(projectDir, cacheDir string) *SyncModule {
	return &SyncModule{projectDir: projectDir, cacheDir: cacheDir}
}

func (m *SyncModule) Name() string { return "sync" }

// SetActivityCallback implements ActivityReporter. cb is invoked whenever a
// batch of filesystem events arrives, even if nothing in it is reindexable —
// any change under the watched tree counts as the project being worked on.
func (m *SyncModule) SetActivityCallback(cb func()) { m.onActivity = cb }

func (m *SyncModule) Start(ctx context.Context) error {
	// Filesystem notifications replace the previous `git status` poll: the poll
	// walked the whole worktree every 5s per project and reported a change up to
	// ~6s late, while the watcher is idle until something happens and names the
	// exact paths — which lets the AST reindex skip discovery entirely
	// (~350ms of a ~1.07s incremental on a 35k-file repository).
	//
	// Two consumers, two ignore files, one watch.
	//
	// The AST honours .astignore and the wiki honours .wikiignore, each applied
	// inside its own pipeline. But there is only one watcher, and it used to be
	// built from the AST checker alone — so .astignore silently decided whether
	// the wiki got told anything at all. Putting docs/ in .astignore, which is a
	// reasonable thing to do since the AST does parse markdown, meant the
	// directory was never watched and editing a document rebuilt nothing.
	//
	// The watch now covers the union of what the two care about, and each
	// consumer applies its own file to what arrives. Both checkers exclude the
	// brand directory by default, so the union still excludes it and the daemon
	// does not wake itself up on its own writes.
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

// ignoreUnion watches whatever any member wants watched: a path is skipped only
// when every member skips it. Routing the survivors to the right consumer is
// classifyBatch's job, using each consumer's own checker.
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

// batchTargets names the indexers a batch has to reach. The two are
// independent, not alternatives: .yaml, .json, .xml and .proto are indexed by
// both the AST pipeline and the knowledge wiki, so one path can set both.
// Markdown is not among them — no shipped query file claims .md, so a document
// reaches the wiki and nothing else.
type batchTargets struct {
	astChanged []string
	astRemoved []string
	knowledge  bool
}

// classifyBatch routes each path in a batch to the indexers that own it.
//
// AST ownership follows the extension and nothing else, which is exactly how a
// full pipeline scan decides it: a scan indexes docs/schema.proto, so an
// incremental update of that same file has to as well, or full and incremental
// runs disagree about what is in the index.
//
// Knowledge ownership needs the path to be under the docs directory — or to be
// one of the documents the scope names explicitly, which is how the root README
// reaches the wiki when docs_dir is docs/ — *and* to carry an extension the wiki
// indexes. Location on its own cannot decide it: knowledge.docs_dir can be set to
// ".", which makes every file in the project "under docs".
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

			// Each consumer applies its own ignore file. The watch is the union
			// of what they want, so a path can arrive that only one of them
			// claims.
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
				// The parse cache is keyed by repo-relative path.
				t.astRemoved = append(t.astRemoved, filepath.ToSlash(rel))
				continue
			}
			t.astChanged = append(t.astChanged, p)
		}
	}
	classify(batch.Changed, false)
	classify(batch.Removed, true)

	return t
}

// handleBatch reindexes whatever the batch touched. AST and knowledge are
// dispatched independently so a docs-only edit does not reparse code, and vice
// versa.
func (m *SyncModule) handleBatch(ctx context.Context, batch fswatch.Batch,
	astIgnore, wikiIgnore fswatch.Ignorer) {
	projectCfg := loadProjectConfigFromDir(m.projectDir)

	scope := knowledge.ScopeFor(m.projectDir, nil, projectCfg)
	docsPath := filepath.Join(m.projectDir, scope.Subdir)
	targets := classifyBatch(batch, m.projectDir, docsPath,
		config.ResolveKnowledgeExtensions(nil, projectCfg), astIgnore, wikiIgnore, scope.ExtraFiles)

	// Events dropped by the kernel leave the index inconsistent with the tree, and
	// only a full scan restores the picture — so a rescan is work for both indexers
	// no matter what the batch happens to name.
	astWork := !config.IsModuleDisabled("ast", nil, projectCfg) &&
		(batch.Rescan || len(targets.astChanged) > 0 || len(targets.astRemoved) > 0)
	knowledgeWork := (targets.knowledge || batch.Rescan) &&
		!config.IsModuleDisabled("knowledge", nil, projectCfg)
	if !astWork && !knowledgeWork {
		return
	}

	// Both reindexers size their pools from the shared CPU budget, which is a budget
	// for one pipeline — and this process runs one supervisor per active project, so
	// without the gate N active projects claimed N times the machine. The slot is
	// taken once for the whole batch rather than once per reindexer: a batch that
	// touches code and docs should not go back to the end of the queue halfway
	// through work it already holds a slot to do.
	askedAt := time.Now()
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		// Shutting down or being parked. The next batch redoes this, and a rescan
		// is what a restarted watcher issues anyway.
		return
	}
	defer release()

	if waited := time.Since(askedAt); waited > time.Second {
		slog.Info("daemon: waited for the indexing slot",
			"project", m.projectDir, "waited", waited.Round(time.Millisecond))
	}

	if astWork {
		if batch.Rescan {
			slog.Warn("daemon: watcher lost events, running a full AST scan", "project", m.projectDir)
			m.reindexAST(ctx, projectCfg, nil, nil)
		} else {
			m.reindexAST(ctx, projectCfg, targets.astChanged, targets.astRemoved)
		}
	}

	if knowledgeWork {
		m.reindexKnowledge(ctx, projectCfg)
	}
}

// ignores reports whether a checker rejects a project-relative path.
func ignores(ig fswatch.Ignorer, relPath string) bool {
	return ig != nil && ig.IsIgnored(relPath, false)
}

// insideDotDir reports whether any directory component of a project-relative
// path starts with a dot.
//
// Full-scan discovery skips dot-directories outright (see collectFiles), and the
// incremental path has to skip them for the same reason plus a sharper one: the
// daemon writes its shards into .graphit inside the very tree it watches, and a
// shard is a .json file, for which there is a parser. Indexing one makes the
// pipeline emit a shard for that shard, whose write is another event, and the
// batch grows without bound.
//
// Only directory components are tested — discovery skips dot-directories, not
// dotfiles, so a parseable .hidden.sql at the root is still source.
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

// isUnder reports whether path is inside dir.
func isUnder(path, dir string) bool {
	rel, err := filepath.Rel(dir, path)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

// reindexAST reindexes the project. When changed/removed are supplied the
// pipeline skips discovery and touches only those paths; passing nil for both
// runs a full scan.
func (m *SyncModule) reindexAST(ctx context.Context, projectCfg config.ConfigMap, changed, removed []string) {
	cfg := ast.LadybugConfigFor(m.projectDir)
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	rebuildLog, closeLog := projectRebuildLogger(m.projectDir)
	defer closeLog()

	revEdges := config.ResolveHubIcebugReverseEdges(nil, projectCfg)
	pipeOpts := ast.PipelineOptions{
		Workers:          ast.SafeWorkers(0),
		IndexSource:      config.ResolveIndexSource(nil, projectCfg),
		CacheDir:         cfg.StoreDir,
		ReverseEdges:     &revEdges,
		GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
		Logger:           rebuildLog,
	}
	var perr error
	if len(changed) > 0 || len(removed) > 0 {
		_, perr = ast.RunPipelineForPaths(ctx, db, m.projectDir, changed, removed, pipeOpts)
	} else {
		_, perr = ast.RunPipeline(ctx, db, m.projectDir, pipeOpts)
	}
	if perr != nil {
		slog.Error("daemon: AST pipeline failed", "store", cfg.StoreDir, "error", perr)
	}
}

func (m *SyncModule) reindexKnowledge(ctx context.Context, projectCfg config.ConfigMap) {
	scope := knowledge.ScopeFor(m.projectDir, nil, projectCfg)

	// Nothing to index is not the same as an empty docs tree: a project can have
	// no docs/ yet and still have a README, which the wiki carries. Bail only when
	// neither exists, so the pipeline is not run to discover that.
	if _, err := os.Stat(filepath.Join(m.projectDir, scope.Subdir)); err != nil && len(scope.ExtraFiles) == 0 {
		return
	}

	wikiDir := store.KnowledgeProjectDir(m.projectDir)
	kCfg := knowledge.IndexConfig{UseLouvain: false, ProjectCfg: projectCfg, Scope: scope}
	_, _ = knowledge.RunIndexPipeline(ctx, m.projectDir, wikiDir, kCfg)
}

// projectRebuildLogger writes to the project's own daemon.log, the file the supervisor
// already appends its lifecycle lines to.
//
// Without it PipelineOptions.Logger stayed nil, and slogutil.Resolve(nil) returns a NOP
// handler that DISCARDS every record. That is why a rebuild that failed its File COPY —
// leaving a graph with no File nodes and no source for any path — left no trace
// anywhere: the error was logged to a logger that threw it away. The lifecycle lines in
// daemon.log came from the supervisor, not from the pipeline, which is what made the log
// look like it was working.
//
// Returns a NOP logger and a no-op closer when the file cannot be opened: losing logs is
// not a reason to skip the reindex.
func projectRebuildLogger(projectDir string) (*slog.Logger, func()) {
	dir := brand.ProjectRuntimePath(projectDir, "daemon")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return slog.New(slog.NewTextHandler(io.Discard, nil)), func() {}
	}
	f, err := os.OpenFile(filepath.Join(dir, "daemon.log"),
		os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return slog.New(slog.NewTextHandler(io.Discard, nil)), func() {}
	}
	logger := slog.New(slog.NewTextHandler(f, &slog.HandlerOptions{Level: slog.LevelInfo})).
		With("module", "ast")
	return logger, func() { _ = f.Close() }
}
