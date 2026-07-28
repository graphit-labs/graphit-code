package daemon

import (
	"context"
	"fmt"
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
}

func NewSyncModule(projectDir, cacheDir string) *SyncModule {
	return &SyncModule{projectDir: projectDir, cacheDir: cacheDir}
}

func (m *SyncModule) Name() string { return "sync" }

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
// independent, not alternatives: .md, .yaml, .json and .xml are indexed by both
// the AST pipeline and the knowledge wiki, so one path can set both.
type batchTargets struct {
	astChanged []string
	astRemoved []string
	knowledge  bool
}

// classifyBatch routes each path in a batch to the indexers that own it.
//
// AST ownership follows the extension and nothing else, which is exactly how a
// full pipeline scan decides it: a scan indexes docs/guia.md, so an incremental
// update of that same file has to as well, or full and incremental runs
// disagree about what is in the index.
//
// Knowledge ownership needs the path to be under the docs directory *and* to
// carry an extension the wiki indexes. Location on its own cannot decide it —
// knowledge.docs_dir defaults to ".", which makes every file in the project
// "under docs".
func classifyBatch(batch fswatch.Batch, projectDir, docsPath string, knowledgeExts map[string]bool,
	astIgnore, wikiIgnore fswatch.Ignorer) batchTargets {
	var t batchTargets

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
			if knowledgeExts[ext] && isUnder(p, docsPath) &&
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

	docsPath := filepath.Join(m.projectDir, config.ResolveDocsDir(nil, projectCfg))
	targets := classifyBatch(batch, m.projectDir, docsPath,
		config.ResolveKnowledgeExtensions(nil, projectCfg), astIgnore, wikiIgnore)

	if !config.IsModuleDisabled("ast", nil, projectCfg) {
		switch {
		case batch.Rescan:
			// Events were dropped by the kernel; only a full scan restores a
			// consistent picture.
			slog.Warn("daemon: watcher lost events, running a full AST scan", "project", m.projectDir)
			m.reindexAST(ctx, projectCfg, nil, nil)
		case len(targets.astChanged) > 0 || len(targets.astRemoved) > 0:
			m.reindexAST(ctx, projectCfg, targets.astChanged, targets.astRemoved)
		}
	}

	if (targets.knowledge || batch.Rescan) && !config.IsModuleDisabled("knowledge", nil, projectCfg) {
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
	cfg := ast.LadybugConfig{
		DBPath: filepath.Join(m.projectDir, brand.DotDir(), "ast", "project", "ladybugdb"),
	}
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	if err := ast.CreateGraphSchema(ctx, db); err != nil {
		slog.Error("daemon: failed to create graph schema", "path", cfg.DBPath, "error", err)
	}

	pipeOpts := ast.PipelineOptions{
		Workers:          ast.SafeWorkers(0),
		IndexSource:      config.ResolveIndexSource(nil, projectCfg),
		CacheDir:         filepath.Dir(cfg.DBPath),
		GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
	}
	var perr error
	if len(changed) > 0 || len(removed) > 0 {
		_, perr = ast.RunPipelineForPaths(ctx, db, m.projectDir, changed, removed, pipeOpts)
	} else {
		_, perr = ast.RunPipeline(ctx, db, m.projectDir, pipeOpts)
	}
	if perr != nil {
		slog.Error("daemon: AST pipeline failed", "path", cfg.DBPath, "error", perr)
	}
}

func (m *SyncModule) reindexKnowledge(ctx context.Context, projectCfg config.ConfigMap) {
	docsDir := config.ResolveDocsDir(nil, projectCfg)
	docsPath := filepath.Join(m.projectDir, docsDir)

	if _, err := os.Stat(docsPath); err != nil {
		return
	}

	wikiDir := filepath.Join(m.projectDir, brand.DotDir(), "knowledge", "project")
	kCfg := knowledge.IndexConfig{UseLouvain: false}
	_, _ = knowledge.RunIndexPipeline(ctx, docsPath, wikiDir, kCfg)
}
