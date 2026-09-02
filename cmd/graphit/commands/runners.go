package commands

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	_ "github.com/graphit-labs/graphit-code/internal/ast/cypher" // registers AI Cypher generator
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/textslice"
	"github.com/graphit-labs/graphit-code/internal/uiserver"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"github.com/graphit-labs/graphit-code/internal/wikisvc"
)

func newASTBackend() (ast.GraphDB, error) {
	cfg := ast.DefaultLadybugConfig()
	return ast.NewLadybugDB(cfg), nil
}

func newASTBackendReadOnly() (ast.GraphDB, error) {
	cfg := ast.DefaultLadybugConfig()
	if _, err := os.Stat(cfg.IcebugDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("no AST database found — index first with: %s ast index", brand.BinName())
	}
	return ast.NewLadybugDBReadOnly(cfg), nil
}

func newASTBackendForContext(name string) (ast.GraphDB, error) {
	cfg := ast.LadybugConfigForContext(name)
	return ast.NewLadybugDB(cfg), nil
}

func newASTBackendForContextReadOnly(name string) (ast.GraphDB, error) {
	cfg := ast.LadybugConfigForContext(name)
	return ast.NewLadybugDBReadOnly(cfg), nil
}

func runModuleRuleSet(module, filePath, ideName string, unset bool) error {
	p := output.NewPrinter("")
	p.Running("Managing %s rule...", module)

	rulesDir := brand.GlobalRulesDir()
	if rulesDir == "" {
		return fmt.Errorf("cannot determine home directory")
	}
	if err := os.MkdirAll(rulesDir, 0o755); err != nil {
		return fmt.Errorf("creating rules dir: %w", err)
	}
	dest := filepath.Join(rulesDir, module+".md")

	if unset {
		if err := os.Remove(dest); err != nil {
			if os.IsNotExist(err) {
				p.Info("No custom rule for %q — nothing to remove.", module)
				return nil
			}
			return fmt.Errorf("removing custom rule: %w", err)
		}
		p.Success("Custom rule for %q removed — default will be used", module)
	} else {
		if filePath == "" {
			return fmt.Errorf("path to rule file is required (or use --unset to remove)")
		}
		data, err := os.ReadFile(filePath)
		if err != nil {
			return fmt.Errorf("reading %s: %w", filePath, err)
		}
		if err := os.WriteFile(dest, data, 0o644); err != nil {
			return fmt.Errorf("writing custom rule: %w", err)
		}
		p.Success("Custom rule for %q saved → %s", module, dest)
	}

	wd, _ := os.Getwd()
	if wd != "" {
		projectCfg := loadProjectConfigFromDir(wd)
		if config.IsModuleDisabled(module, nil, projectCfg) {
			p.StepWarn("Module %q is currently disabled — rule will not be injected until re-enabled", module)
		}
		refreshModuleRule(module, wd, ideName)
	}
	return nil
}

func refreshModuleRule(module, projectDir, ideName string) {
	if projectDir == "" {
		projectDir, _ = os.Getwd()
	}
	if projectDir == "" {
		return
	}

	projectCfg, lockfileIDEs := loadProjectLockInfoFromDir(projectDir)

	if ideName == "" {
		ideName = config.ResolveProjectIDE("", nil, projectCfg, lockfileIDEs)
	}

	disabled := config.IsModuleDisabled(module, nil, projectCfg)

	var err error
	switch module {
	case "knowledge":
		if disabled {
			_ = knowledge.RemoveRule(projectDir, ideName)
			err = knowledge.InstallSkill(projectDir, ideName)
		} else {
			err = knowledge.InstallRule(projectDir, ideName)
		}
	case "ast":
		if disabled {
			_ = ast.RemoveRule(projectDir, ideName)
			err = ast.InstallSkill(projectDir, ideName)
		} else {
			err = ast.InstallRule(projectDir, ideName)
		}
	case "memory":
		if disabled {
			_ = memory.RemoveRule(projectDir, ideName)
			err = memory.InstallSkill(projectDir, ideName)
		} else {
			err = memory.InstallRule(projectDir, ideName)
		}
	case "hub":
		if disabled {
			_ = hub.RemoveRule(projectDir, ideName)
			err = hub.InstallSkill(projectDir, ideName)
		} else {
			err = hub.InstallRule(projectDir, ideName)
		}
	}
	_ = err
}

func getModuleDefaultRule(module string) string {
	switch module {
	case "ast":
		return ast.ASTRuleContent()
	case "knowledge":
		contexts := knowledge.InstalledContexts()
		docsDir := config.ResolveDocsDir(nil, loadProjectConfig())
		return knowledge.KnowledgeRuleContent(contexts, docsDir)
	case "hub":
		return hub.HubRuleContent()
	case "memory":
		contexts := memory.AllContextDirs()
		return memory.RuleContent(contexts)
	default:
		return ""
	}
}

func getModuleResolvedRule(module string) string {
	switch module {
	case "ast":
		return brand.ResolveModuleRule("ast", ast.ASTRuleContent())
	case "knowledge":
		contexts := knowledge.InstalledContexts()
		docsDir := config.ResolveDocsDir(nil, loadProjectConfig())
		return brand.ResolveModuleRule("knowledge", knowledge.KnowledgeRuleContent(contexts, docsDir))
	case "hub":
		return brand.ResolveModuleRule("hub", hub.HubRuleContent())
	case "memory":
		contexts := memory.AllContextDirs()
		return brand.ResolveModuleRule("memory", memory.RuleContent(contexts))
	default:
		return ""
	}
}

// progressThrottle decides when a progress callback is allowed to print.
type progressThrottle struct {
	every time.Duration
	last  time.Time
	phase string
}

// allow reports whether this callback should print. A phase change always
// prints, however recently the last line went out: that is the part carrying
// information — the silence has a new reason.
func (t *progressThrottle) allow(phase string, now time.Time) bool {
	if phase == t.phase && now.Sub(t.last) < t.every {
		return false
	}
	t.phase, t.last = phase, now
	return true
}

// progressInterval says how often the reporter may speak.
//
// On a terminal the line is rewritten in place, so refreshing it costs nothing
// but a redraw and the counter can move like a counter. Redirected to a file,
// a pipe, or the daemon log there is no cursor to move: every refresh is
// another line, and 36k files at this rate would be a log of nothing else. So
// the coarse interval stays exactly where it was for that case.
func progressInterval(tty bool) time.Duration {
	if tty {
		return 200 * time.Millisecond
	}
	return 10 * time.Second
}

// indexProgressReporter turns the pipeline's per-file callback into a progress
// line — one that is overwritten on a terminal, and appended anywhere else.
//
// The pipeline has emitted progress for a long time and nothing consumed it, so
// `ast index` printed the grammar overrides and then nothing at all until it
// finished — 16 minutes of silence on a 36k-file repository, indistinguishable
// from a hang, which is exactly how a real one was missed.
//
// Throttled by time rather than by file count: file cost varies by four orders
// of magnitude here (a 200-line PL/SQL package against a 1.3 M-line XML), so
// "every N files" is silent for minutes on the slow ones and a flood on the
// fast ones. A phase change always prints, however recently the last line went
// out, because that is the part that says the silence has a new reason.
func indexProgressReporter(p *output.Printer) func(string, int, int, int) {
	var (
		mu sync.Mutex
		th = progressThrottle{every: progressInterval(output.IsTTY())}
	)
	return func(ph string, current, total, errors int) {
		mu.Lock()
		defer mu.Unlock()
		if !th.allow(ph, time.Now()) {
			return
		}
		switch ph {
		case "saving-cache":
			p.StepProgress("Saving parse cache: %d file(s)", total)
		case "writing":
			// current is rows already copied into the graph; it is 0 on the single
			// line emitted before the phase starts.
			if current > 0 {
				p.StepProgress("Writing graph: %d row(s) from %d file(s)", current, total)
				return
			}
			p.StepProgress("Writing graph: %d file(s)", total)
		case "searching":
			p.StepProgress("Building search index: %d file(s)", total)
		case "search-maintenance":
			p.StepProgress("Maintaining search index")
		default:
			if errors > 0 {
				p.StepProgress("Parsing: %d/%d file(s), %d error(s)", current, total, errors)
				return
			}
			p.StepProgress("Parsing: %d/%d file(s)", current, total)
		}
	}
}

func runASTIndex(targetPaths []string, workers int, reset bool, reindex bool, cluster string, clusterPaths []string, noSource bool, grammar string) error {
	p := output.NewPrinter("")

	// Parse cluster-path mappings
	clusterPathMap := make(map[string]string)
	for _, cp := range clusterPaths {
		parts := strings.SplitN(cp, "=", 2)
		if len(parts) != 2 {
			return fmt.Errorf("invalid --cluster-path format: %q (expected path=cluster)", cp)
		}
		path := strings.TrimRight(parts[0], "/") + "/"
		clusterPathMap[path] = parts[1]
	}

	// Resolve absolute paths
	absPaths := make([]string, len(targetPaths))
	for i, tp := range targetPaths {
		abs, err := filepath.Abs(tp)
		if err != nil {
			return fmt.Errorf("invalid path %q: %w", tp, err)
		}
		absPaths[i] = abs
	}

	if reset {
		p.Step("Resetting entire database...")
		storeDir := ast.DefaultLadybugConfig().StoreDir
		_ = os.RemoveAll(storeDir)
		p.StepOK("Database reset complete")
	}

	db, err := newASTBackend()
	if err != nil {
		return fmt.Errorf("backend init: %w", err)
	}
	defer func() { _ = db.Close() }()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGTERM, syscall.SIGINT)
	go func() {
		<-sigCh
		p.Warn("Interrupted — saving progress…")
		cancel()
	}()

	// Resolve grammar overrides: config (base) + flag (higher priority)
	projectCfg := loadProjectConfig()
	grammarOverrides := config.ResolveGrammarOverrides(nil, projectCfg)
	if grammar != "" {
		flagOverrides := config.ParseGrammarOverrides(grammar)
		grammarOverrides = config.MergeGrammarOverrides(grammarOverrides, flagOverrides)
	}
	if len(grammarOverrides) > 0 {
		p.Step("Grammar overrides: %v", grammarOverrides)
	}

	// Load cluster map from config if not provided via CLI
	if len(clusterPathMap) == 0 {
		configClusterMap := config.ResolveClusterPathMap(nil, projectCfg)
		for path, cl := range configClusterMap {
			clusterPathMap[path] = cl
		}
	}

	// Build display string for cluster info (after loading from config)
	var clusterInfo strings.Builder
	if len(clusterPathMap) > 0 {
		clusterInfo.WriteString(" (clusters: ")
		first := true
		for path, cl := range clusterPathMap {
			if !first {
				clusterInfo.WriteString(", ")
			}
			fmt.Fprintf(&clusterInfo, "%s=%s", path, cl)
			first = false
		}
		if cluster != "" {
			fmt.Fprintf(&clusterInfo, ", default=%s", cluster)
		}
		clusterInfo.WriteString(")")
	} else if cluster != "" {
		fmt.Fprintf(&clusterInfo, " (cluster: %s)", cluster)
	}

	if len(absPaths) == 1 {
		p.Running("Indexing %s%s", absPaths[0], clusterInfo.String())
	} else {
		p.Running("Indexing %d paths%s", len(absPaths), clusterInfo.String())
	}

	ladybugCfg := ast.DefaultLadybugConfig()

	indexSource := config.ResolveIndexSource(nil, nil)
	if noSource {
		indexSource = false
	}

	// Persist cluster settings to config if provided via CLI
	if len(clusterPaths) > 0 || cluster != "" {
		if err := persistClusterConfig(clusterPathMap, cluster); err != nil {
			p.StepWarn("Failed to persist cluster config: %v", err)
		}
	}

	revEdges := config.ResolveHubIcebugReverseEdges(nil, projectCfg)
	pipeOpts := ast.PipelineOptions{
		Workers:          workers,
		IndexSource:      indexSource,
		CacheDir:         ladybugCfg.StoreDir,
		Cluster:          cluster,
		ClusterPathMap:   clusterPathMap,
		ForceRebuild:     reset || reindex,
		ReverseEdges:     &revEdges,
		GrammarOverrides: grammarOverrides,
		OnProgress:       indexProgressReporter(p),
	}

	// DECISION: incremental vs. full is NOT decided here. It used to be — an `isIncremental`
	// flag stat'ed the icebug directory and picked one of two branches — but the two branches
	// had become byte-for-byte identical, because the only thing that actually distinguishes
	// the two runs is `ForceRebuild: reset || reindex` above, which the pipeline reads for
	// itself. Keeping the fork meant a reader had to diff forty lines to discover it said
	// nothing.
	//
	// The pipeline root is the working directory in both cases, so shard paths and cache
	// keys are relative to the project root and a later incremental run finds them.
	wd, _ := os.Getwd()
	projectRoot := wd

	var allFiles []string
	for _, absPath := range absPaths {
		files, e := collectFilesForPath(absPath, projectRoot)
		if e != nil {
			return fmt.Errorf("collecting files for %s: %w", absPath, e)
		}
		allFiles = append(allFiles, files...)
	}

	pipeOpts.ChangedPaths = make([]string, len(allFiles))
	for i, f := range allFiles {
		rel, _ := filepath.Rel(projectRoot, f)
		pipeOpts.ChangedPaths[i] = rel
	}
	pipeOpts.DeletedPaths = []string{}

	finalResult, err := ast.RunPipelineForPaths(ctx, db, projectRoot, pipeOpts.ChangedPaths, pipeOpts.DeletedPaths, pipeOpts)

	// The last progress line is transient and nothing after an error path would erase it,
	// so EndProgress runs BEFORE the error is returned, not after.
	p.EndProgress()
	if err != nil {
		return err
	}

	totalErrors := finalResult.ErrorCount + finalResult.WriteErrorCount
	if totalErrors > 0 {
		p.Warn("Completed with %d error(s) out of %d files", totalErrors, finalResult.TotalFiles)
	} else if finalResult.SearchIndexRebuilt {
		p.Success("%d files up to date; search index was empty and was rebuilt from cache (%.1fs)",
			finalResult.TotalFiles, finalResult.WriteTime.Seconds())
	} else if finalResult.ParsedFiles == 0 && finalResult.TotalFiles > 0 && finalResult.WriteTime == 0 {
		p.Success("%d files up to date (no changes detected)", finalResult.TotalFiles)
	} else if finalResult.ParsedFiles == 0 && finalResult.WriteTime > 0 {
		p.Success("DB rebuilt from cache (%d files, %.1fs write, %.1fs total)",
			finalResult.TotalFiles, finalResult.WriteTime.Seconds(), finalResult.TotalTime.Seconds())
	} else {
		p.Success("%d files indexed (%.1fs parse, %.1fs write, %.1fs total)",
			finalResult.ParsedFiles, finalResult.ParseTime.Seconds(), finalResult.WriteTime.Seconds(), finalResult.TotalTime.Seconds())
	}

	if finalResult.DiscoverTime > 0 || finalResult.HashTime > 0 {
		p.Step("Timing: discover %.2fs, hash %.2fs, parse %.2fs, cache-save %.2fs, write %.2fs",
			finalResult.DiscoverTime.Seconds(), finalResult.HashTime.Seconds(),
			finalResult.ParseTime.Seconds(), finalResult.WritePhases.CacheSave.Seconds(),
			finalResult.WriteTime.Seconds())
	}
	if finalResult.WriteTime > 0 {
		phases := finalResult.WritePhases
		searchPhase := "search-build"
		if phases.SearchOverlapped {
			searchPhase = "search-wait (build overlapped)"
		}
		p.Step("Write breakdown: graph-preload %.2fs (overlapped), graph-prepare %.2fs, graph-export %.2fs, graph-publish %.2fs, embeddings %.2fs, search-open %.2fs, %s %.2fs, search-maintain %.2fs, search-close %.2fs",
			phases.GraphPreload.Seconds(), phases.GraphPrepare.Seconds(), phases.GraphExport.Seconds(), phases.GraphPublish.Seconds(),
			phases.EmbeddingCache.Seconds(), phases.SearchOpen.Seconds(), searchPhase, phases.SearchBuild.Seconds(),
			phases.SearchMaintain.Seconds(), phases.SearchClose.Seconds())
		if phases.SearchSetup > 0 || phases.SearchPrepare > 0 {
			overlapNote := ""
			if phases.SearchOverlapped {
				overlapNote = " (active work, overlapped)"
			}
			p.Step("Search breakdown%s: setup %.2fs, prepare %.2fs, files-write %.2fs, entities-write %.2fs, files-fts %.2fs, files-scalar %.2fs, entities-fts %.2fs, entities-scalar %.2fs, publish %.2fs",
				overlapNote,
				phases.SearchSetup.Seconds(), phases.SearchPrepare.Seconds(), phases.SearchFilesWrite.Seconds(),
				phases.SearchEntitiesWrite.Seconds(), phases.SearchFilesFTS.Seconds(), phases.SearchFilesScalar.Seconds(),
				phases.SearchEntitiesFTS.Seconds(), phases.SearchEntitiesScalar.Seconds(), phases.SearchPublish.Seconds())
		}
	}

	if finalResult.TimeoutCount > 0 {
		p.StepWarn("Timeouts: %d file(s)", finalResult.TimeoutCount)
	}
	if finalResult.ErrorCount > 0 {
		p.StepWarn("Parse errors: %d file(s)", finalResult.ErrorCount)
		for i := 0; i < len(finalResult.ErrorFiles); i++ {
			p.ListItem("%s", finalResult.ErrorFiles[i])
		}
	}
	if finalResult.WriteErrorCount > 0 {
		p.StepWarn("Write errors: %d chunk(s)", finalResult.WriteErrorCount)
		for i := 0; i < len(finalResult.WriteErrorFiles); i++ {
			p.ListItem("%s", finalResult.WriteErrorFiles[i])
		}
	}
	if finalResult.EmptyCount > 0 {
		p.Step("Empty (0 entities): %d file(s)", finalResult.EmptyCount)
		for i := 0; i < len(finalResult.EmptyFiles); i++ {
			p.ListItem("%s", finalResult.EmptyFiles[i])
		}
	}

	if len(finalResult.EngineStats) > 0 {
		p.Step("Breakdown:")

		keys := make([]string, 0, len(finalResult.EngineStats))
		for k := range finalResult.EngineStats {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			p.ListItem("%s — %d file(s)", k, finalResult.EngineStats[k])
		}
	}

	return nil
}

// persistClusterConfig saves the cluster and cluster-path settings to the project config.
func persistClusterConfig(clusterPathMap map[string]string, defaultCluster string) error {
	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getwd: %w", err)
	}
	lockPath := filepath.Join(wd, "graphit.lock.json")

	lf, err := hub.LoadLockfile(lockPath)
	if err != nil {
		return fmt.Errorf("load lockfile: %w", err)
	}
	if lf == nil {
		return nil // no lockfile, nothing to persist
	}

	if lf.Config == nil {
		lf.Config = make(map[string]any)
	}

	// Use nested structure for config
	astConfig, ok := lf.Config["ast"].(map[string]any)
	if !ok {
		astConfig = make(map[string]any)
	}

	if len(clusterPathMap) > 0 {
		// Build comma-separated string
		var pairs []string
		for path, cluster := range clusterPathMap {
			pairs = append(pairs, fmt.Sprintf("%s=%s", path, cluster))
		}
		astConfig["cluster_map"] = strings.Join(pairs, ",")
	}

	if defaultCluster != "" {
		astConfig["cluster"] = defaultCluster
	}

	lf.Config["ast"] = astConfig

	if err := hub.SaveLockfile(lockPath, lf); err != nil {
		return fmt.Errorf("save lockfile: %w", err)
	}
	return nil
}

// collectFilesForPath collects all parseable files under a path, honouring the
// project's ignore rules (.gitignore and .astignore).
//
// This is the CLI's own discovery pass, beyond the pipeline's: the command feeds the
// pipeline a scoped ChangedPaths list, so the pipeline's desktop (collectFiles) never
// runs — which means the ignore checker must be applied HERE or nothing is ignored.
//
// What a directory is excluded by is the ignore rules alone — including dot-directories,
// which are listed there. No kind of path is skipped structurally.
//
// projectRoot is the boundary the ignore rules are anchored to and collected up to:
// the project being indexed, which may be a parent of a scoped path like
// `ast index internal/ui`. The scoped path is only where the walk starts.
func collectFilesForPath(rootPath, projectRoot string) ([]string, error) {
	ic := ignorer.DirScope(ast.NewAstIgnoreChecker(projectRoot))
	var files []string
	err := filepath.Walk(rootPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // skip errors
		}
		rel, relErr := filepath.Rel(projectRoot, path)
		if relErr != nil {
			return nil
		}
		if info.IsDir() {
			if rel != "." && ic.IsIgnored(rel, true) && !ic.ShouldDescend(rel) {
				return filepath.SkipDir
			}
			// The directory's own ignore files (.gitignore/.astignore in it)
			// apply to whatever lives under it — git semantics, so a
			// `.opencode/.gitignore` with `node_modules` scopes to .opencode.
			if rel != "." {
				ic = ic.At(rel)
			}
			return nil
		}
		if ic.IsIgnored(rel, false) {
			return nil
		}
		ext := strings.ToLower(filepath.Ext(path))
		// Check if we have a parser for this extension
		if ast.HasParserForExtensionIn(rootPath, ext) {
			files = append(files, path)
		}
		return nil
	})
	return files, err
}

func runASTWatch(targetPath string, workers int, cluster string, clusterPaths []string) error {
	p := output.NewPrinter("")

	absPath, err := filepath.Abs(targetPath)
	if err != nil {
		return fmt.Errorf("invalid path: %w", err)
	}

	// Parse cluster-path mappings
	clusterPathMap := make(map[string]string)
	for _, cp := range clusterPaths {
		parts := strings.SplitN(cp, "=", 2)
		if len(parts) != 2 {
			return fmt.Errorf("invalid --cluster-path format: %q (expected path=cluster)", cp)
		}
		path := strings.TrimRight(parts[0], "/") + "/"
		clusterPathMap[path] = parts[1]
	}

	// Load cluster map from config if not provided via CLI
	if len(clusterPathMap) == 0 {
		projectCfg := loadProjectConfig()
		configClusterMap := config.ResolveClusterPathMap(nil, projectCfg)
		for path, cl := range configClusterMap {
			clusterPathMap[path] = cl
		}
	}

	// Persist cluster settings to config if provided via CLI
	if len(clusterPaths) > 0 || cluster != "" {
		if err := persistClusterConfig(clusterPathMap, cluster); err != nil {
			p.StepWarn("Failed to persist cluster config: %v", err)
		}
	}

	db, err := newASTBackend()
	if err != nil {
		return fmt.Errorf("backend init: %w", err)
	}
	defer func() { _ = db.Close() }()

	projectCfg := loadProjectConfig()

	cfg := ast.DefaultWatcherConfig()
	if workers > 0 {
		cfg.Workers = workers
	}
	cfg.Cluster = cluster
	cfg.ClusterPathMap = clusterPathMap
	cfg.GrammarOverrides = config.ResolveGrammarOverrides(nil, projectCfg)

	watcher, err := ast.NewWatcher(db, absPath, cfg)
	if err != nil {
		return fmt.Errorf("watcher init: %w", err)
	}

	// Build display string for cluster info
	var clusterInfo strings.Builder
	if len(clusterPathMap) > 0 {
		clusterInfo.WriteString(" (clusters: ")
		first := true
		for path, cl := range clusterPathMap {
			if !first {
				clusterInfo.WriteString(", ")
			}
			fmt.Fprintf(&clusterInfo, "%s=%s", path, cl)
			first = false
		}
		if cluster != "" {
			fmt.Fprintf(&clusterInfo, ", default=%s", cluster)
		}
		clusterInfo.WriteString(")")
	} else if cluster != "" {
		fmt.Fprintf(&clusterInfo, " (cluster: %s)", cluster)
	}

	p.Info("Watching %s for changes... [tree-sitter]%s", absPath, clusterInfo.String())
	p.Step("Press Ctrl+C to stop")

	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	return watcher.Start(ctx)
}

func runUnifiedServe(repoPath string) error {
	p := output.NewPrinter("")

	ide := os.Getenv(brand.EnvVar("IDE"))
	if ide == "" {
		ide = config.FallbackIDE
	}

	if repoPath == "" {
		repoPath, _ = os.Getwd()
	}

	ctx := context.Background()
	reg, err := hub.NewRegistryManager(ctx)
	if err != nil {
		p.StepWarn("Hub registry unavailable — running in offline mode")
		reg, _ = hub.NewRegistryManager(ctx)
	}
	hubSvc := hub.NewHubService(reg)

	astDB, err := newASTBackendReadOnly()
	if err != nil {
		astDB, err = newASTBackend()
		if err != nil {
			return fmt.Errorf("ast backend: %w", err)
		}
	}
	defer func() { _ = astDB.Close() }()

	projectName := filepath.Base(repoPath)
	lockPath := filepath.Join(repoPath, brand.LockFileName())
	if data, err := os.ReadFile(lockPath); err == nil {
		var lockData map[string]any
		if json.Unmarshal(data, &lockData) == nil {
			if proj, ok := lockData["project"].(map[string]any); ok {
				if name, ok := proj["name"].(string); ok && name != "" {
					projectName = name
				}
			}
		}
	}

	srv, err := uiserver.NewUnifiedServer(hubSvc, ide, astDB, repoPath, projectName)
	if err != nil {
		return err
	}

	url := fmt.Sprintf("http://localhost:%d", srv.Port())
	p.Success("Graphit UI → %s", url)
	p.Step("Hub + AST available on this port")
	p.Step("Press Ctrl+C to stop")

	go func() {
		time.Sleep(300 * time.Millisecond)
		openBrowser(url)
	}()

	sigCtx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	return srv.Start(sigCtx)
}

func runASTQuery(query string, contextName string, aiMode bool, cypherOnly bool, aiOptimized bool) error {
	p := output.NewPrinter("")

	var db ast.GraphDB
	var err error
	if contextName != "" {
		db, err = newASTBackendForContextReadOnly(contextName)
		p.Info("Querying context: %s", contextName)
	} else {
		db, err = newASTBackendReadOnly()
	}
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	ctx := context.Background()

	if aiMode {
		aiClient, err := ai.NewClientFromConfig()
		if err != nil {
			return err
		}

		resp, err := ast.GenerateAICypher(ctx, db, aiClient, ast.AICypherRequest{
			UserQuery:  query,
			MaxResults: 25,
			Backend:    db.BackendType(),
		})
		if err != nil {
			return fmt.Errorf("AI generation failed: %w", err)
		}

		if cypherOnly {

			p.Data(resp.Cypher)
			return nil
		}

		p.Step("Generated Cypher:")
		p.Data(resp.Cypher)
		p.Blank()

		if resp.Error != "" {
			p.Warn("Query execution error: %s", resp.Error)
			return nil
		}

		if resp.Result == nil || len(resp.Result.Records) == 0 {
			p.Info("No results.")
			return nil
		}

		if aiOptimized {
			p.Data(ast.FormatRecordsTOON(resp.Result.Records))
		} else {
			out, _ := json.MarshalIndent(resp.Result.Records, "", "  ")
			p.Data(string(out))
		}
		p.Count("record", len(resp.Result.Records))
		return nil
	}

	result, err := db.Query(ctx, query, nil)
	if err != nil {
		return err
	}

	if len(result.Records) == 0 {
		p.Info("No results.")
		return nil
	}

	if aiOptimized {
		p.Data(ast.FormatRecordsTOON(result.Records))
	} else {
		out, _ := json.MarshalIndent(result.Records, "", "  ")
		p.Data(string(out))
	}
	p.Count("record", len(result.Records))
	return nil
}

func runASTHybridSearch(query string, contextName string, topK int, aiOptimized bool) error {
	p := output.NewPrinter("")

	var db ast.GraphDB
	var err error
	if contextName != "" {
		db, err = newASTBackendForContextReadOnly(contextName)
		p.Info("Hybrid search in context: %s", contextName)
	} else {
		db, err = newASTBackendReadOnly()
	}
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	qs := ast.NewQueryService(db)

	embClient, embErr := ai.NewEmbeddingClientFromConfig()
	if embErr == nil {
		qs.SetEmbeddingClient(embClient)
		if topK > 0 {
			p.Running("Hybrid search: %q (model: %s, top %d)", query, embClient.ModelName(), topK)
		} else {
			p.Running("Hybrid search: %q (model: %s)", query, embClient.ModelName())
		}
	} else {
		if topK > 0 {
			p.Running("Hybrid search: %q (FTS-only, top %d)", query, topK)
		} else {
			p.Running("Hybrid search: %q (FTS-only, no embedding client)", query)
		}
	}

	results, err := qs.HybridSearch(ctx, query, topK)
	if err != nil {
		return fmt.Errorf("hybrid search failed: %w", err)
	}

	if len(results) == 0 {
		p.Info("No results. Indexes may not be built yet — re-index with: graphit ast index . && graphit ast embed")
		return nil
	}

	if aiOptimized {
		records := make([]ast.QueryRecord, len(results))
		for i, r := range results {
			records[i] = ast.QueryRecord{
				"type":  r.Type,
				"name":  r.Name,
				"path":  r.Path,
				"line":  r.Line,
				"score": r.RelevanceScore,
			}
		}
		p.Data(ast.FormatRecordsTOON(records))
	} else {
		out, _ := json.MarshalIndent(results, "", "  ")
		p.Data(string(out))
	}
	p.Count("result", len(results))
	return nil
}

func runASTImportList() error {
	p := output.NewPrinter("")

	contexts := ast.ListImportedContexts()
	if len(contexts) == 0 {
		p.Info("No imported contexts.")
		return nil
	}

	p.Header("Imported AST Contexts (%d)", len(contexts))
	for key, ictx := range contexts {
		label := ictx.Name
		if label == "" {
			label = key
		}
		detail := ictx.SourcePath
		if detail == "" {
			detail = ictx.StoreDir
		}
		p.Step("%s", label)
		p.Detail("Source Path", detail)
	}
	return nil
}

func runASTImport(sourcePath, name string, reset bool, workers int) error {
	p := output.NewPrinter("ast:import")

	absPath, err := filepath.Abs(sourcePath)
	if err != nil {
		return fmt.Errorf("invalid path: %w", err)
	}

	wd, _ := os.Getwd()
	ictx, err := ast.AddImportedContext(wd, name, absPath)
	if err != nil {
		return fmt.Errorf("register context: %w", err)
	}

	task := p.StartTask("Importing %s as context %q...", absPath, name)
	p.Step("Store: %s", ictx.StoreDir)

	if reset {
		task.Update("Resetting context store...")
		_ = os.RemoveAll(ictx.StoreDir)
		p.StepOK("Context store reset")
	}

	db, err := newASTBackendForContext(name)
	if err != nil {
		task.Fail("Backend init: %v", err)
		return fmt.Errorf("backend init: %w", err)
	}
	defer func() { _ = db.Close() }()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGTERM, syscall.SIGINT)
	go func() {
		<-sigCh
		p.Warn("Interrupted — saving progress…")
		cancel()
	}()

	task.Update("Indexing files...")
	pc := loadProjectConfig()
	rev := config.ResolveHubIcebugReverseEdges(nil, pc)
	pipeOpts := ast.PipelineOptions{
		Workers:          workers,
		IndexSource:      true,
		CacheDir:         ictx.StoreDir,
		ReverseEdges:     &rev,
		GrammarOverrides: config.ResolveGrammarOverrides(nil, loadProjectConfig()),
	}

	result, err := ast.RunPipeline(ctx, db, absPath, pipeOpts)
	if err != nil {
		task.Fail("Indexing failed: %v", err)
		return err
	}

	totalErrors := result.ErrorCount + result.WriteErrorCount
	if totalErrors > 0 {
		task.Done("Completed with %d error(s) out of %d files", totalErrors, result.TotalFiles)
		if result.ErrorCount > 0 {
			p.StepWarn("Parse errors: %d file(s)", result.ErrorCount)
			for _, e := range result.ErrorFiles {
				p.ListItem("%s", e)
			}
		}
		if result.WriteErrorCount > 0 {
			p.StepWarn("Write errors: %d chunk(s)", result.WriteErrorCount)
			for _, e := range result.WriteErrorFiles {
				p.ListItem("%s", e)
			}
		}
	} else {
		task.Done("Context %q: %d files indexed (%.1fs)", name, result.ParsedFiles, result.TotalTime.Seconds())
	}

	{
		ms, msErr := memory.NewMemoryStore()
		if msErr == nil {
			memsvc := memory.NewMemoryServiceForContext(name, ms)
			if err := memsvc.SyncWiki(); err != nil {
				p.StepWarn("Memory context %q: %v", name, err)
			} else {
				p.Step("Memory context compiled from %s", memsvc.TableURI())
			}
			_ = memsvc.Close()
		}
	}
	return nil
}

func runASTExport(format, outputDir string, noSources bool) error {
	p := output.NewPrinter("")

	db, err := newASTBackend()
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	repoPath, _ := os.Getwd()
	absDir, _ := filepath.Abs(outputDir)

	switch format {
	case "obsidian":
		p.Info("Exporting Obsidian vault → %s", absDir)
		exporter := ast.NewObsidianExporter(db, repoPath)
		if err := exporter.Export(context.Background(), absDir); err != nil {
			return err
		}
		p.Success("Exported to %s", absDir)
	case "bundle":
		p.Info("Exporting .ast bundle → %s", absDir)
		opts := ast.BundleOptions{
			StorePath: ast.DefaultLadybugConfig().StoreDir,
			NoSources: noSources,
		}
		if err := ast.ExportBundle(context.Background(), db, repoPath, absDir, opts, nil); err != nil {
			return err
		}
		if noSources {
			p.Success("Exported to %s (structure only, --no-sources)", absDir)
		} else {
			p.Success("Exported to %s (with sources)", absDir)
		}
	default:
		return fmt.Errorf("unsupported format %q (supported: obsidian, bundle)", format)
	}
	return nil
}

func runASTClean(contextName string) error {
	p := output.NewPrinter("")

	if contextName != "" {
		p.Info("Removing imported context: %s", contextName)
		wd, _ := os.Getwd()
		if err := ast.RemoveImportedContext(wd, contextName); err != nil {
			return err
		}
		p.Success("Context %q removed", contextName)
		return nil
	}

	db, err := newASTBackend()
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	if _, err := db.Execute(ctx, `MATCH (n) DETACH DELETE n`, nil); err != nil {
		return fmt.Errorf("clean: %w", err)
	}

	p.Success("Project graph cleared")
	return nil
}

func runASTSource(relPath, contextName, entity, entityType string, head, tail, startLine, endLine int, pattern string, isRegex bool, before, after int, lineNumbers bool) error {
	p := output.NewPrinter("")

	var cfg ast.LadybugConfig
	if contextName != "" {
		cfg = ast.LadybugConfigForContext(contextName)
	} else {
		cfg = ast.DefaultLadybugConfig()
	}

	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	svc := ast.NewSourceService(db).WithStore(cfg.StoreDir)
	result, err := svc.GetSource(ctx, ast.SourceRequest{
		Path:        relPath,
		Entity:      entity,
		EntityType:  entityType,
		Head:        head,
		Tail:        tail,
		StartLine:   startLine,
		EndLine:     endLine,
		Pattern:     pattern,
		IsRegex:     isRegex,
		Before:      before,
		After:       after,
		LineNumbers: lineNumbers,
	})
	if err != nil {
		return err
	}

	if result.Entity != nil {
		p.Step("%s %s (lines %d-%d)", result.Entity.Type, result.Entity.Name, result.Entity.StartLine, result.Entity.EndLine)
	}

	if result.Source == "" && len(result.Matches) == 0 {
		p.Warn("No matches found for pattern %q in %s", pattern, relPath)
		return nil
	}

	p.Data(result.Source)
	return nil
}

func runKnowledgeSync() error {
	p := output.NewPrinter("")

	wd, err := os.Getwd()
	if err != nil {
		return err
	}
	projectCfg := loadProjectConfig()
	scope := knowledge.ScopeFor(wd, nil, projectCfg)
	p.Running("Re-indexing knowledge wiki from %s/…", scope.Subdir)
	return runKnowledgeIndex(wd, scope, 0, false, false)
}

func runKnowledgeList() error {
	p := output.NewPrinter("")
	p.ListItem("project (local)")
	contexts := knowledge.InstalledContexts()
	for _, name := range contexts {
		p.ListItem("%s", name)
	}
	p.Count("knowledge context", len(contexts)+1)
	return nil
}

func openKnowledgeForRead(ctx context.Context, contextName string) (*wiki.WikiDB, string, error) {
	wd, err := os.Getwd()
	if err != nil {
		return nil, "", err
	}
	if contextName != "" {
		if rec, ok := store.LookupContext(wd, store.KindKnowledge, contextName); ok && rec.IsHub() {
			st, err := hub.NewS3Store(ctx, nil, loadProjectConfig())
			if err != nil {
				return nil, "", fmt.Errorf("opening Hub store: %w", err)
			}
			mount, ok := st.MountedWikiFor(wd, contextName)
			if !ok {
				return nil, "", fmt.Errorf("knowledge context %q is a Hub artifact, but its object store is not configured", contextName)
			}
			db, err := wiki.OpenWikiDBAt(ctx, mount.Config)
			if err != nil {
				return nil, "", fmt.Errorf("opening knowledge artifact %s@%s: %w", mount.ArtifactID, mount.Version, err)
			}
			if !db.HasContent(ctx) {
				_ = db.Close()
				return nil, "", fmt.Errorf("knowledge artifact %s@%s has no indexed content", mount.ArtifactID, mount.Version)
			}
			return db, mount.Config.URI, nil
		}
	}

	wikiDir := knowledge.WikiDirForContextIn(wd, contextName)
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, "", fmt.Errorf("opening knowledge wiki: %w", err)
	}
	if !db.HasContent(ctx) {
		_ = db.Close()
		return nil, "", fmt.Errorf("knowledge wiki at %s has no indexed content", wikiDir)
	}
	return db, wikiDir, nil
}

func runKnowledgeSearch(term string, contextName string) error {
	ctx := context.Background()
	p := output.NewPrinter("")

	db, _, err := openKnowledgeForRead(ctx, contextName)
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	results := wiki.BM25SearchFrom(ctx, db, term, 0)
	if len(results) == 0 {
		p.Info("No knowledge matching %q.", term)
		return nil
	}

	for _, r := range results {
		p.ListItem("[%.3f] %s — %s", r.Score, strings.TrimSuffix(r.Path, ".md"), r.Title)
	}
	p.Count("match", len(results))
	return nil
}

func runKnowledgeQuery(query string, contextName string) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	db, _, err := openKnowledgeForRead(ctx, contextName)
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	aiClient, err := ai.NewClientFromConfig()
	if err != nil {
		return fmt.Errorf("AI not configured: %w", err)
	}

	p.Running("Searching knowledge wiki…")
	result, err := wiki.SearchWikiFrom(ctx, aiClient, db, query, wiki.SearchConfig{
		ModuleTag: "knowledge",
		UseBM25:   true,
	})
	if err != nil {
		return err
	}

	p.Step("Consulted %d wiki page(s)", result.Turns)
	p.Blank()
	p.Data(result.Answer)
	return nil
}

// runKnowledgeIndex builds the wiki for root. scope names what under root is
// read — the configured docs tree plus the root README for a project index, and
// nothing (meaning "all of it") when the caller pointed at a directory directly.
func runKnowledgeIndex(root string, scope knowledge.WikiScope, workers int, reset, useLouvain bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	target := root
	if scope.Subdir != "" && scope.Subdir != "." {
		target = filepath.Join(root, scope.Subdir)
	}
	task := p.StartTask("Indexing %s into knowledge wiki...", target)

	wikiDir := knowledge.WikiDir()
	cfg := knowledge.IndexConfig{
		Workers:    workers,
		Reset:      reset,
		BatchSize:  100,
		UseLouvain: useLouvain,
		Scope:      scope,
	}

	result, err := knowledge.RunIndexPipeline(ctx, root, wikiDir, cfg)
	if err != nil {
		task.Fail("Indexing failed: %v", err)
		return fmt.Errorf("index failed: %w", err)
	}

	task.Done("Wiki generated: %d articles → %s (took %s)",
		result.IndexedFiles, wikiDir, result.TotalTime.Round(time.Millisecond))
	return nil
}

func runKnowledgeClean() error {
	p := output.NewPrinter("")
	wikiDir := knowledge.WikiDir()
	if err := os.RemoveAll(wikiDir); err != nil {
		return fmt.Errorf("removing wiki dir: %w", err)
	}
	_ = os.MkdirAll(wikiDir, 0o755)
	p.Success("Knowledge wiki cleared")
	return nil
}

func runKnowledgeLint(contextName string, staleDays int) error {
	p := output.NewPrinter("")
	ctx := context.Background()
	db, source, err := openKnowledgeForRead(ctx, contextName)
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	p.Running("Auditing knowledge wiki…")

	cfg := wiki.LintConfig{StaleDays: staleDays}

	report, err := wiki.LintWikiFrom(ctx, db, source, cfg)
	if err != nil {
		return fmt.Errorf("lint failed: %w", err)
	}

	if report.HasIssues() {
		p.Warn("%s", report.Summary())
	} else {
		p.Success("%s", report.Summary())
	}

	if len(report.Orphans) > 0 {
		p.Header("Orphan Pages (%d)", len(report.Orphans))
		for _, o := range report.Orphans {
			p.ListItem("%s.md", o)
		}
	}
	if len(report.BrokenLinks) > 0 {
		p.Header("Broken Links (%d)", len(report.BrokenLinks))
		for _, bl := range report.BrokenLinks {
			p.ListItem("[[%s]] in %s.md", bl.Target, bl.Source)
		}
	}
	if len(report.StalePages) > 0 {
		p.Header("Stale Pages (%d)", len(report.StalePages))
		for _, s := range report.StalePages {
			p.ListItem("%s.md", s)
		}
	}
	if len(report.EmptyPages) > 0 {
		p.Header("Empty Pages (%d)", len(report.EmptyPages))
		for _, e := range report.EmptyPages {
			p.ListItem("%s.md", e)
		}
	}
	if len(report.MissingFields) > 0 {
		p.Header("Missing Frontmatter (%d)", len(report.MissingFields))
		for _, fi := range report.MissingFields {
			p.ListItem("%s.md — missing: %s", fi.Page, strings.Join(fi.MissingFields, ", "))
		}
	}

	return nil
}

func runASTVerify(contextName string) error {
	p := output.NewPrinter("")

	repoPath, err := os.Getwd()
	if err != nil {
		return err
	}

	var db ast.GraphDB
	if contextName != "" {
		db, err = newASTBackendForContext(contextName)
		p.Info("Verifying context: %s", contextName)
	} else {
		db, err = newASTBackend()
	}
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	report, err := ast.VerifyGraphAgainstDisk(context.Background(), db, repoPath)
	if err != nil {
		return err
	}

	p.Data(ast.FormatVerifyReport(report))
	if !report.Clean() {
		// A non-zero status so this can gate a pipeline. The message already said
		// what to do, so the error stays short.
		return fmt.Errorf("%d node(s) hold text their file does not contain", len(report.Divergences))
	}
	return nil
}

func runASTSchema(contextName string) error {
	p := output.NewPrinter("")

	var db ast.GraphDB
	var err error
	if contextName != "" {
		db, err = newASTBackendForContext(contextName)
		p.Info("Schema for context: %s", contextName)
	} else {
		db, err = newASTBackend()
	}
	if err != nil {
		return err
	}
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	schemaText, err := ast.SchemaText(ctx, db)
	if err != nil {
		return err
	}

	p.Data(schemaText)
	return nil
}

func newMemorySvc(userScope bool) (*memory.MemoryService, string, error) {
	var scope memory.MemoryScope
	var scopeID string

	if userScope {
		scope = memory.MemoryScopeUser
		hash, err := memory.UserScopeID()
		if err != nil {
			return nil, "", err
		}
		scopeID = hash
	} else {
		scope = memory.MemoryScopeProject
		lf, err := hub.LoadLockfile(lockfilePath())
		if err != nil || lf == nil {
			return nil, "", fmt.Errorf("project not initialised — run '%s init' first", brand.BinName())
		}
		scopeID = lf.Project.ID
	}

	projectID := ""
	if lf, err := hub.LoadLockfile(lockfilePath()); err == nil && lf != nil {
		projectID = lf.Project.ID
	}

	ms, _ := memory.NewMemoryStore()

	svc := memory.NewMemoryService(scope, scopeID, ms)

	if err := svc.EnsureInitialised(); err != nil {

		_ = err
	}

	return svc, projectID, nil
}

func newMemorySvcForContext(contextName string) (*memory.MemoryService, error) {
	ms, err := memory.NewMemoryStore()
	if err != nil {
		return nil, fmt.Errorf("memory store not available: %w", err)
	}
	svc := memory.NewMemoryServiceForContext(contextName, ms)
	if err := svc.EnsureInitialised(); err != nil {
		_ = err
	}
	return svc, nil
}

func runMemoryAdd(title, content string, userScope, linkProject, important bool, memType string, tags string) error {
	p := output.NewPrinter("")

	svc, projectID, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	if memType != "" && !memory.ValidMemoryType(memType) {
		return fmt.Errorf("invalid memory type %q — valid types: convention, correction, decision, tension, fact, skill", memType)
	}

	assocProject := ""
	if userScope && linkProject {
		assocProject = projectID
	}

	var tagList []string
	if tags != "" {
		for _, t := range strings.Split(tags, ",") {
			t = strings.TrimSpace(t)
			if t != "" {
				tagList = append(tagList, t)
			}
		}
	}

	slug, err := svc.AddMemory(title, content, memory.MemoryOpts{
		ProjectID: assocProject,
		Important: important,
		Type:      memory.MemoryType(memType),
		Tags:      tagList,
	})
	if err != nil {
		return err
	}

	scopeLabel := "project"
	if userScope {
		scopeLabel = "user"
		if linkProject && assocProject != "" {
			scopeLabel += " (linked to project)"
		}
	}
	if important {
		scopeLabel += " [important]"
	}
	if memType != "" {
		scopeLabel += " [" + memType + "]"
	}
	p.Success("Memory %q saved [%s]", slug, scopeLabel)
	p.Step("Wiki: %s", svc.WikiDir())

	refreshModuleRule("memory", "", "")
	return nil
}

func runMemoryUpdate(id, content, title string, userScope bool) error {
	p := output.NewPrinter("")

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	if err := svc.UpdateMemory(id, title, content); err != nil {
		return err
	}
	p.Success("Memory %q updated", id)

	refreshModuleRule("memory", "", "")
	return nil
}

func runMemorySearch(term string, userScope bool) error {
	ctx := context.Background()
	p := output.NewPrinter("")

	scope := "project"
	if userScope {
		scope = "user"
	}

	wikiDir := memory.WikiDir(scope)
	if wikiDir == "" {
		p.Info("No memories found in %s scope.", scope)
		return nil
	}

	results := memory.SearchChains(ctx, wikiDir, term, 0)
	if len(results) == 0 {
		p.Info("No memories matching %q in %s scope.", term, scope)
		return nil
	}

	for _, r := range results {
		slug := strings.TrimSuffix(r.Path, ".md")
		if r.Superseded {
			p.ListItem("[%.3f] %s — %s (superseded — current: %s)", r.Score, slug, r.Title, r.Current)
			continue
		}
		p.ListItem("[%.3f] %s — %s", r.Score, slug, r.Title)
	}
	p.Count("match", len(results))
	return nil
}

func runMemoryRemove(slug string, userScope bool) error {
	p := output.NewPrinter("")

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	if err := svc.RemoveMemory(slug); err != nil {
		return err
	}
	p.Success("Memory %q removed", slug)

	refreshModuleRule("memory", "", "")
	return nil
}

func runMemoryList(userScope bool) error {
	p := output.NewPrinter("")

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	memories, err := svc.ListMemories()
	if err != nil {
		return err
	}
	if len(memories) == 0 {
		p.Info("No memories found. Run '%s memory update' to fetch from hub.", brand.BinName())
		return nil
	}
	for _, m := range memories {
		ts := m.CreatedAt
		if ts != "" {
			ts = " (" + ts + ")"
		}
		importantTag := ""
		if m.Important {
			importantTag = " ★"
		}
		p.ListItem("[%s] %s%s%s", m.ID, m.Title, importantTag, ts)
	}
	p.Count("memory", len(memories))
	return nil
}

func runMemoryIndex(userScope, reset bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	// --reset clears the wiki before indexing, matching `ast index --reset` and
	// `knowledge index --reset`.
	//
	// It exists because an ordinary index cannot repair an index that is wrong for a
	// reason other than a changed memory. The normal incremental path compares the source table
	// with the wiki table; --reset deliberately discards the derived side before compiling it again.
	//
	// Nothing discarded here is source. The memories live in their own store, outside the
	// wiki; the chunks and the vectors are all derived from them.
	if reset {
		if _, rerr := wiki.ResetDir(svc.WikiDir()); rerr != nil {
			return fmt.Errorf("clearing the memory wiki: %w", rerr)
		}
		p.Step("Cleared %s", svc.WikiDir())
	}

	task := p.StartTask("Indexing memories from %s...", svc.TableURI())
	if err := svc.IndexMemories(ctx); err != nil {
		task.Fail("Memory indexing failed: %v", err)
		return err
	}
	task.Done("Memory index complete. Wiki: %s", svc.WikiDir())
	return nil
}

func runMemoryQuery(query string, userScope bool, contextName string) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	var wikiDir string
	var scopeLabel string
	switch {
	case contextName != "":
		wikiDir = memory.WikiDir(contextName)
		scopeLabel = contextName
	case userScope:
		wikiDir = memory.WikiDir("user")
		scopeLabel = "user"
	default:
		wikiDir = memory.WikiDir("project")
		scopeLabel = "project"
	}
	if wikiDir == "" {
		p.Info("Memory not initialized for %s scope. Run '%s init' first.", scopeLabel, brand.BinName())
		return nil
	}

	aiClient, err := ai.NewClientFromConfig()
	if err != nil {
		return fmt.Errorf("AI not configured: %w", err)
	}

	p.Running("Searching memory wiki [%s]…", scopeLabel)
	result, err := wiki.SearchWiki(ctx, aiClient, query, wiki.SearchConfig{
		WikiDir:   wikiDir,
		ModuleTag: "memory",
	})
	if err != nil {
		return err
	}

	p.Step("Consulted %d wiki page(s)", result.Turns)
	p.Blank()
	p.Data(result.Answer)
	return nil
}

func runMemoryImport(projectID string) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	svc, err := newMemorySvcForContext(projectID)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	task := p.StartTask("Importing external memory context %q...", projectID)
	if err := svc.SyncWiki(); err != nil {
		task.Fail("Sync failed: %v", err)
		return err
	}
	task.Update("Indexing memories...")
	if err := svc.IndexMemories(ctx); err != nil {
		p.StepWarn("Indexing failed: %v", err)
	}
	task.Done("Import complete. Wiki: %s", svc.WikiDir())
	p.Step("Query with: %s memory query \"...\" --context %s", brand.BinName(), projectID)
	refreshModuleRule("memory", "", "")
	return nil
}

func runMemoryImportantList(userScope bool) error {
	p := output.NewPrinter("")

	scope := "project"
	if userScope {
		scope = "user"
	}

	entries, err := memory.ListImportantMemories(scope)
	if err != nil {
		return err
	}
	if len(entries) == 0 {
		p.Info("No important memories found in %s scope.", scope)
		return nil
	}

	for _, e := range entries {
		p.ListItem("[%s] %s", e.ID, e.Title)
		if e.Content != "" {
			p.Data(e.Content)
			p.Blank()
		}
	}
	p.Count("important memory", len(entries))
	return nil
}

func runMemoryPromote(id string, userScope bool) error {
	p := output.NewPrinter("")

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	if err := svc.PromoteMemory(id); err != nil {
		return err
	}
	p.Success("Memory %q promoted to important", id)

	refreshModuleRule("memory", "", "")
	return nil
}

func runMemoryDemote(id string, userScope bool) error {
	p := output.NewPrinter("")

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	if err := svc.DemoteMemory(id); err != nil {
		return err
	}
	p.Success("Memory %q demoted (no longer important)", id)

	refreshModuleRule("memory", "", "")
	return nil
}

func runMemoryClean() error {
	p := output.NewPrinter("")

	wikiDir := memory.WikiDir("project")
	if wikiDir == "" {
		p.Info("Memory not initialized. Run '%s init' first.", brand.BinName())
		return nil
	}
	if err := os.RemoveAll(wikiDir); err != nil {
		return fmt.Errorf("removing memory wiki: %w", err)
	}
	_ = os.MkdirAll(wikiDir, 0o755)
	p.Success("Project memory wiki cleared. The authoritative memory table was preserved.")
	p.Step("Rebuild: %s memory index", brand.BinName())
	return nil
}

func runMemoryRemoveContext(contextName string) error {
	p := output.NewPrinter("")
	// An imported memory context is a prefix of the shared memory store, so what is
	// dropped is this machine's copy of it: its local table directory and its compiled wiki,
	// both global. The remote prefix survives — another unit may still be reading it — and
	// there is no project-local copy left to remove.
	if err := os.RemoveAll(memory.TableDirFor(contextName, contextName)); err != nil {
		return fmt.Errorf("removing memory context table: %w", err)
	}
	if err := os.RemoveAll(memory.MemoryWikiGlobalDir(contextName, contextName)); err != nil {
		return fmt.Errorf("removing memory context wiki: %w", err)
	}
	p.Success("Removed memory context %q", contextName)
	wd, _ := os.Getwd()
	refreshModuleRule("memory", wd, "")
	return nil
}

func runMemorySchema(contextName string) error {
	p := output.NewPrinter("")
	p.Header("Memory Table Schema")
	p.Info("Primary key: key (live: <id>; revision: <id>/<revision-id>)")
	p.Info("Core columns: id, revision_id, superseded, title, body, type, tags_json")
	p.Info("Lifecycle columns: created_at, updated_at, revision, previous, next, updated_by")
	p.Info("Scope columns: scope, scope_id, project_id; vector column: embedding")
	return nil
}

func runMemorySync(contextName string) error {
	p := output.NewPrinter("")
	svc, err := newMemorySvcForContext(contextName)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()
	task := p.StartTask("Syncing memory context %q...", contextName)
	if err := svc.SyncWiki(); err != nil {
		task.Fail("Sync failed: %v", err)
		return err
	}
	task.Done("Sync complete")
	return nil
}

func runMemoryConsolidate(userScope, dryRun bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	scope := "project"
	if userScope {
		scope = "user"
	}

	// The same agent CLI the dream module uses. Consolidation only needs judgement
	// back as text — which memories duplicate or contradict which — so the
	// analytical client is the right one; every mutation is applied by Go under the
	// invariants in ApplyConsolidation.
	aiClient, aiErr := ai.NewClientFromConfig()
	if aiErr != nil {
		p.Warn("No AI CLI found — only the deterministic staleness check will run.")
		p.Info("Set one with '%s config ai.cli <binary>' to get duplicate and contradiction analysis.", brand.BinName())
	}

	task := p.StartTask("Analysing %s memories...", scope)
	report, err := memory.RunConsolidation(ctx, scope, aiClient)
	if err != nil {
		task.Fail("Analysis failed: %v", err)
		return err
	}
	task.Done("Analysed %d memories", report.TotalMemories)

	if report.AIFailed {
		p.Warn("The analysis step failed, so only the deterministic checks ran.")
		if report.AIAnalysis != "" {
			p.Info("%s", report.AIAnalysis)
		}
	}

	if !report.HasActions() {
		p.Success("Nothing to consolidate — no duplicates, contradictions or stale entries found")
		return nil
	}

	printConsolidationPlan(p, report)

	if dryRun {
		p.Blank()
		p.Info("Dry run — nothing was changed. Re-run with --dry-run=false to apply.")
		return nil
	}

	svc, _, svcErr := newMemorySvc(userScope)
	if svcErr != nil {
		return svcErr
	}
	defer func() { _ = svc.Close() }()

	outcome, err := memory.ApplyConsolidation(ctx, scope, report, svc)
	if err != nil {
		return err
	}

	p.Blank()
	p.Header("Applied (%d)", len(outcome.Applied))
	for _, a := range outcome.Applied {
		if len(a.Removed) > 0 {
			p.ListItem("%s → kept %s, folded in %s", a.Type, a.Kept, strings.Join(a.Removed, ", "))
			continue
		}
		p.ListItem("%s → %s", a.Type, a.Kept)
	}

	// Refusals are the interesting output: they are where the invariants stopped a
	// proposal, and each one is work for a human or an agent with more context.
	if len(outcome.Skipped) > 0 {
		p.Blank()
		p.Header("Refused (%d)", len(outcome.Skipped))
		for _, a := range outcome.Skipped {
			p.ListItem("%s [%s] — %s", a.Type, a.Kept, a.Skipped)
		}
	}
	if len(outcome.Failed) > 0 {
		p.Blank()
		p.Header("Failed (%d)", len(outcome.Failed))
		for _, a := range outcome.Failed {
			p.ListItem("%s [%s] — %s", a.Type, a.Kept, a.Err)
		}
	}

	p.Blank()
	p.Success("%d applied, %d refused, %d failed", len(outcome.Applied), len(outcome.Skipped), len(outcome.Failed))
	if len(outcome.Applied) > 0 {
		refreshModuleRule("memory", "", "")
	}
	return nil
}

func printConsolidationPlan(p *output.Printer, report *memory.ConsolidationReport) {
	section := func(title string, actions []memory.ConsolidationAction) {
		if len(actions) == 0 {
			return
		}
		p.Header("%s (%d)", title, len(actions))
		for _, a := range actions {
			ids := strings.Join(a.MemoryIDs, ", ")
			if a.KeepID != "" && len(a.MemoryIDs) > 1 {
				p.ListItem("keep %s of [%s] — %s", a.KeepID, ids, a.Reason)
				continue
			}
			p.ListItem("[%s] %s", ids, a.Reason)
		}
	}

	section("Duplicates", report.Duplicates)
	section("Contradictions", report.Contradictions)
	section("Suggestions", report.Suggestions)
	section("Stale", report.Stale)

	p.Blank()
	p.Step("%d action(s) proposed", report.TotalActions())
}

func watchAndReindex(rootPath string, useLouvain bool, reindex func() error) error {
	p := output.NewPrinter("")
	if err := reindex(); err != nil {
		p.Warn("Initial index error: %v", err)
	}

	// Filesystem notifications instead of polling `git status` every two
	// seconds: no worktree walk per tick, no git requirement, and changes are
	// picked up immediately. Ignore rules (.gitignore plus the module's own
	// ignore file) are honoured when registering watches and filtering events.
	w, err := fswatch.New(fswatch.Config{
		Root:        rootPath,
		Ignore:      ast.NewAstIgnoreChecker(rootPath),
		Debounce:    500 * time.Millisecond,
		MaxDebounce: 5 * time.Second,
	})
	if err != nil {
		return fmt.Errorf("watch: %w", err)
	}
	defer func() { _ = w.Close() }()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	batches, err := w.Start(ctx)
	if err != nil {
		return fmt.Errorf("watch %s: %w", rootPath, err)
	}

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt, syscall.SIGTERM)

	for {
		select {
		case <-sig:
			output.Interrupted()
			return nil
		case batch, ok := <-batches:
			if !ok {
				return nil
			}
			if len(batch.Changed) == 0 && len(batch.Removed) == 0 && !batch.Rescan {
				continue
			}
			p.Running("Change detected, re-indexing…")
			if err := reindex(); err != nil {
				p.Warn("Re-index error: %v", err)
			} else {
				p.Success("Re-index complete")
			}
		}
	}
}

func runASTSync(contextName string) error {
	p := output.NewPrinter("")

	if contextName == "" {
		return fmt.Errorf("--context is required for ast sync")
	}

	contexts := ast.ListImportedContexts()
	ictx, ok := contexts[contextName]
	if !ok {
		return fmt.Errorf("context %q not found — install it first with: %s ast install <path> --context %s", contextName, brand.BinName(), contextName)
	}

	if _, err := os.Stat(ictx.SourcePath); err != nil {
		return fmt.Errorf("source path %q no longer accessible: %w", ictx.SourcePath, err)
	}

	task := p.StartTask("Syncing AST context %q...", contextName)

	db, err := newASTBackendForContext(contextName)
	if err != nil {
		task.Fail("Backend init: %v", err)
		return fmt.Errorf("backend init: %w", err)
	}
	defer func() { _ = db.Close() }()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	pipeOpts := ast.PipelineOptions{
		IndexSource:      true,
		CacheDir:         ictx.StoreDir,
		GrammarOverrides: config.ResolveGrammarOverrides(nil, loadProjectConfig()),
	}

	result, err := ast.RunPipeline(ctx, db, ictx.SourcePath, pipeOpts)
	if err != nil {
		task.Fail("Sync failed: %v", err)
		return err
	}

	task.Done("Context %q synced: %d files indexed (%.1fs)", contextName, result.ParsedFiles, result.TotalTime.Seconds())
	return nil
}

func runKnowledgeRemoveContext(contextName string) error {
	p := output.NewPrinter("")
	wd, _ := os.Getwd()
	// Only this project's claim. The wiki is global and another project may have
	// imported the same context.
	if err := store.RemoveContext(wd, store.KindKnowledge, contextName); err != nil {
		return fmt.Errorf("removing knowledge context: %w", err)
	}
	p.Success("Removed knowledge context %q", contextName)
	refreshModuleRule("knowledge", wd, "")
	return nil
}

// runKnowledgeWatch watches root and rebuilds the wiki on change. The watch
// covers all of root — the README lives outside the docs tree, and an edit to it
// has to rebuild too — while scope still decides what each rebuild reads.
func runKnowledgeWatch(root string, scope knowledge.WikiScope, useLouvain bool) error {
	p := output.NewPrinter("")
	p.Running("Watching %s for changes…", root)
	return watchAndReindex(root, useLouvain, func() error {
		ctx := context.Background()
		wikiDir := knowledge.WikiDir()
		cfg := knowledge.IndexConfig{UseLouvain: useLouvain, Scope: scope}
		_, err := knowledge.RunIndexPipeline(ctx, root, wikiDir, cfg)
		return err
	})
}

func runKnowledgeSchema(contextName string) error {
	p := output.NewPrinter("")
	p.Header("KNOWLEDGE Wiki")
	p.Info("Wiki directory: %s", knowledge.WikiDir())
	p.Info("Architecture: file-based wiki (no graph database)")
	return nil
}

func runWikiSearch(query string, wikiRefs, hubRefs []string, sessionName string, continueSession bool, topK int, mode string, aiOptimized bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	wikiSvc := wikisvc.NewWikiService(wd)
	wikiSources, resolveErrs := wikiSvc.ResolveSources(ctx, wikiRefs, hubRefs)
	for _, e := range resolveErrs {
		p.Warn("%v", e)
	}

	if len(wikiSources) == 0 {
		return fmt.Errorf("no valid wiki sources resolved — check your --wiki and --hub flags")
	}

	var session *chat.ChatSession
	if continueSession {
		session, err = chat.LatestSession(wd)
		if err != nil {
			p.Warn("No previous session found, creating new one")
			session = nil
		}
	}
	if session == nil {

		chatSources := make([]chat.Source, len(wikiSources))
		for i, ws := range wikiSources {
			chatSources[i] = chat.Source{
				ID:    ws.ID,
				Label: ws.Label,
				Kind:  chat.SourceWiki,
				Dir:   ws.Dir,
			}
		}
		session = chat.NewSession(wd, chatSources, query)
		if sessionName != "" {
			session.Title = sessionName
		}
	}

	if err := session.Append(chat.ChatMessage{
		Role:    "user",
		Content: query,
	}); err != nil {
		p.Warn("Failed to save query to session: %v", err)
	}

	aiClient, err := ai.NewClientFromConfig()
	if err != nil {
		return fmt.Errorf("AI not configured: %w", err)
	}

	p.Running("Searching %d wiki source(s)…", len(wikiSources))
	for _, ws := range wikiSources {
		p.Step("[%s] %s", ws.ID, ws.Label)
	}

	useBM25 := mode != "semantic"

	result, err := wiki.SearchMultiWiki(ctx, aiClient, query, wiki.MultiWikiSearchConfig{
		Sources:           wikiSources,
		UseBM25:           useBM25,
		BM25TopNPerSource: topK,
	})
	if err != nil {
		return err
	}

	p.Step("Consulted %d wiki turn(s)", result.Turns)
	p.Blank()
	p.Data(result.Answer)

	if err := session.Append(chat.ChatMessage{
		Role:    "assistant",
		Content: result.Answer,
	}); err != nil {
		p.Warn("Failed to save answer to session: %v", err)
	}

	p.Blank()
	p.Step("Session: %s", session.ID)
	p.Step("Continue: %s wiki chat --continue", brand.BinName())
	return nil
}

func runWikiChat(sessionID string, continueSession bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	var session *chat.ChatSession
	if sessionID != "" {
		session, err = chat.LoadSession(sessionID)
		if err != nil {
			return fmt.Errorf("loading session %q: %w", sessionID, err)
		}
	} else if continueSession {
		session, err = chat.LatestSession(wd)
		if err != nil {
			return fmt.Errorf("no sessions found: %w", err)
		}
	} else {
		return fmt.Errorf("specify --session <id> or --continue to resume a session")
	}

	p.Info("Resuming session: %s", session.Title)
	p.Step("Session ID: %s | Messages: %d", session.ID, session.MessageCount)
	p.Step("Type /exit or Ctrl+D to end the chat")
	p.Blank()

	aiClient, err := ai.NewClientFromConfig()
	if err != nil {
		return fmt.Errorf("AI not configured: %w", err)
	}

	engine := chat.NewChatEngine(aiClient, session)

	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for {
		fmt.Print("\n> ")
		if !scanner.Scan() {

			p.Blank()
			p.Info("Session saved: %s", session.ID)
			return nil
		}

		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		if line == "/exit" {
			p.Info("Session saved: %s", session.ID)
			return nil
		}

		response, err := engine.Send(ctx, line)
		if err != nil {
			p.Warn("Error: %v", err)
			continue
		}

		p.Blank()
		p.Data(response)
	}
}

func runWikiSessions(deleteID string) error {
	p := output.NewPrinter("")

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	wikiSvc := wikisvc.NewWikiService(wd)

	if deleteID != "" {
		if err := wikiSvc.DeleteSession(deleteID); err != nil {
			return fmt.Errorf("deleting session: %w", err)
		}
		p.Success("Session %s deleted", deleteID)
		return nil
	}

	sessions, err := wikiSvc.ListSessions()
	if err != nil {
		return err
	}

	if len(sessions) == 0 {
		p.Info("No sessions found for this project.")
		p.Step("Start one with: %s wiki search \"your query\"", brand.BinName())
		return nil
	}

	p.Header("Wiki Sessions (%d)", len(sessions))
	for _, s := range sessions {
		title := s.Title
		if title == "" {
			title = "Unnamed Session"
		}
		p.Step("%s (%s)", title, s.ID)
		p.Detail("Last Active", s.UpdatedAt.Format("2006-01-02 15:04:05"))
		p.Detail("Messages", fmt.Sprintf("%d", s.MessageCount))
		if len(s.Sources) > 0 {
			ids := make([]string, len(s.Sources))
			for i, ws := range s.Sources {
				ids[i] = ws.ID
			}
			p.Detail("Sources", strings.Join(ids, ", "))
		}
	}
	return nil
}

// openWikiDBForScope resolves the wiki scope to a directory and opens the WikiDB.
func openWikiDBForScope(ctx context.Context, wikiScope, projectDir string) (*wiki.WikiDB, error) {
	wikiDir, err := wikiDirForScope(wikiScope, "", projectDir)
	if err != nil {
		return nil, err
	}
	if wikiDir == "" {
		return nil, fmt.Errorf("no %s wiki for %s — it has not been built yet", wikiScope, projectDir)
	}
	return wiki.OpenWikiDB(ctx, wikiDir)
}

// wikiDirForScope resolves the directory holding a wiki's pages and index.
//
// No chdir. Every resolver now takes the project explicitly, because the stores are
// global and keyed by identity rather than found by walking the working directory.
func wikiDirForScope(wikiScope, contextName, projectDir string) (string, error) {
	switch wikiScope {
	case "project", "knowledge", "":
		return knowledge.WikiDirForContextIn(projectDir, contextName), nil
	case "memory":
		scope := contextName
		if scope == "" {
			scope = "project"
		}
		return memory.WikiDirFor(projectDir, scope), nil
	default:
		return "", fmt.Errorf("unknown wiki scope %q — use 'project' or 'memory'", wikiScope)
	}
}

func runWikiSource(page, wikiScope, contextName, projectDir string, req textslice.Request) error {
	p := output.NewPrinter("")

	if projectDir == "" {
		wd, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("getting working directory: %w", err)
		}
		projectDir = wd
	}
	abs, err := filepath.Abs(projectDir)
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	wikiDir, err := wikiDirForScope(wikiScope, contextName, abs)
	if err != nil {
		return err
	}

	result, err := wiki.ReadPageAt(context.Background(), wikiDir, page, req)
	if err != nil {
		// Only a mistyped slug is helped by a list of alternatives. A rejected
		// reference — one escaping the wiki directory — needs its own reason kept.
		if errors.Is(err, wiki.ErrPageNotFound) {
			if pages := wiki.ListPagesAt(context.Background(), wikiDir); len(pages) > 0 {
				sort.Strings(pages)
				p.StepWarn("%v", err)
				p.Info("Pages in this wiki:")
				for _, name := range pages {
					p.Step("%s", name)
				}
			}
		}
		return err
	}

	if result.Source == "" && len(result.Matches) == 0 {
		p.Info("No matches found for pattern %q in %s", req.Pattern, result.Page)
		return nil
	}

	p.Data(result.Source)
	return nil
}

// runWikiExport renders a compiled wiki back into Markdown, on demand.
//
// The generators no longer write pages, so this is the only thing that produces them — and it
// produces them where the caller asks rather than inside the wiki directory, which is the
// difference that keeps the index the single artifact.
func runWikiExport(wikiScope, contextName, projectDir, outDir string) error {
	ctx := context.Background()
	p := output.NewPrinter("")

	if projectDir == "" {
		wd, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("getting working directory: %w", err)
		}
		projectDir = wd
	}
	abs, err := filepath.Abs(projectDir)
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	wikiDir, err := wikiDirForScope(wikiScope, contextName, abs)
	if err != nil {
		return err
	}

	moduleTag := "knowledge"
	if wikiScope == "memory" {
		moduleTag = "memory"
	}

	p.Running("Exporting the %s wiki to Markdown…", moduleTag)
	result, err := wiki.ExportMarkdown(ctx, wikiDir, outDir, moduleTag)
	if err != nil {
		return fmt.Errorf("export failed: %w", err)
	}

	p.Success("Exported %d page(s) to %s", result.Pages, result.OutputDir)
	if result.HasLog {
		p.Step("log.md written from the sync history")
	}
	return nil
}

func runWikiBrowse(wikiScope, docType string, limit int, aiOptimized bool) error {
	ctx := context.Background()
	p := output.NewPrinter("")

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	db, err := openWikiDBForScope(ctx, wikiScope, wd)
	if err != nil {
		return fmt.Errorf("opening wiki db: %w", err)
	}
	defer func() { _ = db.Close() }()

	filter := wiki.BrowseFilter{
		DocType:   docType,
		ClusterID: -1,
		Limit:     limit,
	}

	entries, err := db.Browse(ctx, filter)
	if err != nil {
		return fmt.Errorf("browsing wiki: %w", err)
	}

	if len(entries) == 0 {
		p.Info("No wiki documents found.")
		return nil
	}

	if aiOptimized {
		p.Data(wiki.FormatBrowseResultsTOON(entries))
		return nil
	}

	p.Header("Wiki Documents (%d)", len(entries))
	for i, e := range entries {
		typeLabel := e.DocType
		if typeLabel == "" {
			typeLabel = "doc"
		}
		if e.Important {
			p.Step("%d. ⭐ %s [%s] (confidence: %.1f)", i+1, e.Title, typeLabel, e.Confidence)
		} else {
			p.Step("%d. %s [%s] (confidence: %.1f)", i+1, e.Title, typeLabel, e.Confidence)
		}
		if e.Breadcrumb != "" {
			p.Detail("Path", e.Breadcrumb)
		}
		if e.ClusterName != "" {
			p.Detail("Cluster", e.ClusterName)
		}
		if e.Summary != "" {
			summary := e.Summary
			if len(summary) > 120 {
				summary = summary[:120] + "…"
			}
			p.Detail("Summary", summary)
		}
		p.Detail("Words", fmt.Sprintf("%d", e.WordCount))
	}
	return nil
}

func runWikiLog(wikiScope string, limit int, aiOptimized bool) error {
	ctx := context.Background()
	p := output.NewPrinter("")

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	db, err := openWikiDBForScope(ctx, wikiScope, wd)
	if err != nil {
		return fmt.Errorf("opening wiki db: %w", err)
	}
	defer func() { _ = db.Close() }()

	entries, err := db.QuerySyncLog(ctx, limit)
	if err != nil {
		return fmt.Errorf("querying sync log: %w", err)
	}

	if len(entries) == 0 {
		p.Info("No sync history found.")
		return nil
	}

	if aiOptimized {
		p.Data(wiki.FormatSyncLogTOON(entries))
		return nil
	}

	p.Header("Wiki Sync Log (%d entries)", len(entries))
	for _, e := range entries {
		p.Step("#%d — %s", e.ID, e.Timestamp)
		p.Detail("Total docs", fmt.Sprintf("%d", e.TotalDocs))
		p.Detail("Articles written", fmt.Sprintf("%d", e.ArticlesWritten))
		if e.BacklinksAdded > 0 {
			p.Detail("Backlinks added", fmt.Sprintf("%d", e.BacklinksAdded))
		}
		if len(e.Added) > 0 {
			p.Detail("Added", strings.Join(e.Added, ", "))
		}
		if len(e.Updated) > 0 {
			p.Detail("Updated", strings.Join(e.Updated, ", "))
		}
		if len(e.Deleted) > 0 {
			p.Detail("Deleted", strings.Join(e.Deleted, ", "))
		}
	}
	return nil
}

func runWikiXRefs(query, wikiScope string, depth int, aiOptimized bool) error {
	ctx := context.Background()
	p := output.NewPrinter("")

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	db, err := openWikiDBForScope(ctx, wikiScope, wd)
	if err != nil {
		return fmt.Errorf("opening wiki db: %w", err)
	}
	defer func() { _ = db.Close() }()

	if depth < 1 {
		depth = 1
	}
	if depth > 3 {
		depth = 3
	}

	refs, err := db.FindXRefs(ctx, query, depth)
	if err != nil {
		return fmt.Errorf("finding xrefs: %w", err)
	}

	if len(refs) == 0 {
		p.Info("No cross-references found for %q.", query)
		return nil
	}

	if aiOptimized {
		p.Data(wiki.FormatXRefResultsTOON(query, depth, refs))
		return nil
	}

	// Split into outbound and inbound.
	var outbound, inbound []wiki.XRefResult
	for _, r := range refs {
		if r.Direction == "outbound" {
			outbound = append(outbound, r)
		} else {
			inbound = append(inbound, r)
		}
	}

	p.Header("Cross-References for %q (depth %d)", query, depth)

	if len(outbound) > 0 {
		p.Step("Outbound (%d):", len(outbound))
		for _, r := range outbound {
			p.ListItem("→ %s (%s) [%s]", r.Title, r.Slug, r.RefType)
		}
	}

	if len(inbound) > 0 {
		p.Step("Inbound (%d):", len(inbound))
		for _, r := range inbound {
			p.ListItem("← %s (%s) [%s]", r.Title, r.Slug, r.RefType)
		}
	}

	return nil
}

func runWikiEmbed(wikiScope string) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("getting working directory: %w", err)
	}

	// The same targets the daemon and the MCP tool embed. This used to build its own
	// path with a "wiki" subdirectory that does not exist, and since OpenWikiDB
	// creates what it opens, it embedded an empty database it had just created and
	// then printed "All wiki chunks already have embeddings" — a success message
	// about a file with nothing in it.
	targets := daemon.WikiEmbedTargets(wd, nil)
	if wikiScope != "" {
		filtered := targets[:0]
		for _, t := range targets {
			slashed := filepath.ToSlash(t.Dir)
			switch wikiScope {
			case "project", "knowledge":
				if strings.Contains(slashed, "/knowledge/") {
					filtered = append(filtered, t)
				}
			case "memory":
				if strings.Contains(slashed, "/memory/") {
					filtered = append(filtered, t)
				}
			default:
				return fmt.Errorf("unknown wiki scope %q — use 'project' or 'memory'", wikiScope)
			}
		}
		targets = filtered
	}
	if len(targets) == 0 {
		return fmt.Errorf("no wiki to embed for scope %q", wikiScope)
	}

	p.Running("Generating embeddings for %s wiki…", wikiScope)

	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		return fmt.Errorf("embedding client: %w", err)
	}

	embedder := wiki.NewWikiEmbedder(client, wiki.DefaultWikiEmbedConfig())
	total := 0
	for _, target := range targets {
		count, err := embedder.RunCycle(ctx, target.Dir)
		if err != nil {
			return fmt.Errorf("embedding cycle (%s): %w", target.Dir, err)
		}
		total += count
	}

	if total == 0 {
		p.Success("All wiki chunks already have embeddings")
	} else {
		p.Success("Embedded %d wiki chunk(s)", total)
	}

	return nil
}
