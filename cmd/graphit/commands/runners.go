package commands

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
	"time"

	"crypto/sha256"
	"io/fs"

	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	_ "github.com/graphit-labs/graphit-code/internal/ast/cypher" // registers AI Cypher generator
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/improvements"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
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
	if _, err := os.Stat(cfg.DBPath); os.IsNotExist(err) {
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
	case "improvements":
		if disabled {
			_ = improvements.RemoveRule(projectDir, ideName)
			err = improvements.InstallSkill(projectDir, ideName)
		} else {
			err = improvements.InstallRule(projectDir, ideName)
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
		installed := hub.LoadInstalledArtifacts()
		return hub.HubRuleContent(installed)
	case "memory":
		contexts := memory.AllContextDirs()
		return memory.RuleContent(contexts)
	case "improvements":
		return improvements.DefaultRules()
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
		installed := hub.LoadInstalledArtifacts()
		return brand.ResolveModuleRule("hub", hub.HubRuleContent(installed))
	case "memory":
		contexts := memory.AllContextDirs()
		return brand.ResolveModuleRule("memory", memory.RuleContent(contexts))
	case "improvements":
		return improvements.Rules()
	default:
		return ""
	}
}

func runASTIndex(targetPath string, workers int, reset bool, reindex bool, cluster string, noSource bool, grammar string) error {
	p := output.NewPrinter("")

	absPath, err := filepath.Abs(targetPath)
	if err != nil {
		return fmt.Errorf("invalid path: %w", err)
	}

	if reset {
		p.Step("Resetting entire database...")
		ladybugCfg := ast.DefaultLadybugConfig()
		projectDir := filepath.Dir(ladybugCfg.DBPath)
		_ = os.RemoveAll(projectDir)
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

	if reindex && !reset {
		p.Step("Removing previous index for %s...", absPath)
		writer := ast.NewGraphWriter(db, absPath, true)
		if err := writer.DeleteRepository(ctx, absPath); err != nil {
			p.StepWarn("Reindex cleanup: %v", err)
		}
		p.StepOK("Repository data removed")
	}

	if cluster != "" {
		p.Running("Indexing %s (cluster: %s)", absPath, cluster)
	} else {
		p.Running("Indexing %s", absPath)
	}

	ladybugCfg := ast.DefaultLadybugConfig()

	indexSource := config.ResolveIndexSource(nil, nil)
	if noSource {
		indexSource = false
	}

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


	pipeOpts := ast.PipelineOptions{
		Workers:          workers,
		IndexSource:      indexSource,
		CacheDir:         filepath.Dir(ladybugCfg.DBPath),
		Cluster:          cluster,
		ForceRebuild:     reindex,
		GrammarOverrides: grammarOverrides,
	}

	result, err := ast.RunPipeline(ctx, db, absPath, pipeOpts)
	if err != nil {
		return err
	}

	totalErrors := result.ErrorCount + result.WriteErrorCount
	if totalErrors > 0 {
		p.Warn("Completed with %d error(s) out of %d files", totalErrors, result.TotalFiles)
	} else if result.ParsedFiles == 0 && result.TotalFiles > 0 && result.WriteTime == 0 {
		p.Success("%d files up to date (no changes detected)", result.TotalFiles)
	} else if result.ParsedFiles == 0 && result.WriteTime > 0 {
		p.Success("DB rebuilt from cache (%d files, %.1fs write, %.1fs total)",
			result.TotalFiles, result.WriteTime.Seconds(), result.TotalTime.Seconds())
	} else {
		p.Success("%d files indexed (%.1fs parse, %.1fs write, %.1fs total)",
			result.ParsedFiles, result.ParseTime.Seconds(), result.WriteTime.Seconds(), result.TotalTime.Seconds())
	}

	if result.DiscoverTime > 0 || result.HashTime > 0 {
		p.Step("Timing: discover %.2fs, hash %.2fs, parse %.2fs, write %.2fs",
			result.DiscoverTime.Seconds(), result.HashTime.Seconds(),
			result.ParseTime.Seconds(), result.WriteTime.Seconds())
	}

	if result.TimeoutCount > 0 {
		p.StepWarn("Timeouts: %d file(s)", result.TimeoutCount)
	}
	if result.ErrorCount > 0 {
		p.StepWarn("Parse errors: %d file(s)", result.ErrorCount)
		for i := 0; i < len(result.ErrorFiles); i++ {
			p.ListItem("%s", result.ErrorFiles[i])
		}
	}
	if result.WriteErrorCount > 0 {
		p.StepWarn("Write errors: %d chunk(s)", result.WriteErrorCount)
		for i := 0; i < len(result.WriteErrorFiles); i++ {
			p.ListItem("%s", result.WriteErrorFiles[i])
		}
	}
	if result.EmptyCount > 0 {
		p.Step("Empty (0 entities): %d file(s)", result.EmptyCount)
		for i := 0; i < len(result.EmptyFiles); i++ {
			p.ListItem("%s", result.EmptyFiles[i])
		}
	}

	if len(result.EngineStats) > 0 {
		p.Step("Breakdown:")

		keys := make([]string, 0, len(result.EngineStats))
		for k := range result.EngineStats {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			p.ListItem("%s — %d file(s)", k, result.EngineStats[k])
		}
	}

	return nil
}

func runASTWatch(targetPath string, workers int, cluster string) error {
	p := output.NewPrinter("")

	absPath, err := filepath.Abs(targetPath)
	if err != nil {
		return fmt.Errorf("invalid path: %w", err)
	}

	db, err := newASTBackend()
	if err != nil {
		return fmt.Errorf("backend init: %w", err)
	}
	defer func() { _ = db.Close() }()

	_ = ast.CreateGraphSchema(context.Background(), db)

	projectCfg := loadProjectConfig()

	cfg := ast.DefaultWatcherConfig()
	if workers > 0 {
		cfg.Workers = workers
	}
	cfg.Cluster = cluster
	cfg.GrammarOverrides = config.ResolveGrammarOverrides(nil, projectCfg)

	watcher, err := ast.NewWatcher(db, absPath, cfg)
	if err != nil {
		return fmt.Errorf("watcher init: %w", err)
	}

	p.Info("Watching %s for changes... [tree-sitter]", absPath)
	p.Step("Press Ctrl+C to stop")

	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	return watcher.Start(ctx)
}

func runUnifiedServe(repoPath string) error {
	p := output.NewPrinter("")

	ide := os.Getenv(brand.EnvVar("IDE"))
	if ide == "" {
		ide = "claude"
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

	astJobs := ast.NewJobManager()

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

	srv, err := uiserver.NewUnifiedServer(hubSvc, ide, astDB, astJobs, repoPath, projectName)
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
			detail = ictx.DBPath
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

	ictx, err := ast.AddImportedContext(name, absPath)
	if err != nil {
		return fmt.Errorf("register context: %w", err)
	}

	task := p.StartTask("Importing %s as context %q...", absPath, name)
	p.Step("Database: %s", ictx.DBPath)

	if reset {
		task.Update("Resetting context database...")
		contextDir := filepath.Dir(ictx.DBPath)
		_ = os.RemoveAll(contextDir)
		p.StepOK("Context database reset")
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
	pipeOpts := ast.PipelineOptions{
		Workers:          workers,
		IndexSource:      true,
		CacheDir:         filepath.Dir(ictx.DBPath),
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
		ms, msErr := memory.NewMemoryGitStore()
		if msErr == nil {
			memsvc := memory.NewMemoryServiceForContext(name, ms)
			if err := memsvc.SyncToLocal(); err != nil {
				p.StepWarn("Memory context %q: %v", name, err)
			} else {
				p.Step("Memory context synced → %s", memsvc.LocalDir())
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
		if err := ast.ExportBundle(context.Background(), db, repoPath, absDir, nil); err != nil {
			return err
		}
		p.Success("Exported to %s (with sources)", absDir)
	default:
		return fmt.Errorf("unsupported format %q (supported: obsidian, bundle)", format)
	}
	return nil
}

func runASTClean(contextName string) error {
	p := output.NewPrinter("")

	if contextName != "" {
		p.Info("Removing imported context: %s", contextName)
		if err := ast.RemoveImportedContext(contextName); err != nil {
			return err
		}
		p.Success("Context %q removed", contextName)

		wd, _ := os.Getwd()
		memDir := filepath.Join(wd, brand.DotDir(), "memory", contextName)
		if info, statErr := os.Lstat(memDir); statErr == nil {
			var removeErr error
			if info.Mode()&os.ModeSymlink != 0 {
				removeErr = os.Remove(memDir)
			} else {
				removeErr = os.RemoveAll(memDir)
			}
			if removeErr != nil {
				p.StepWarn("Memory context cleanup %q: %v", contextName, removeErr)
			} else {
				p.Step("Memory context removed: %s", memDir)
			}
		}
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
	svc := ast.NewSourceService(db)
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

func runKnowledgeSync(contextName string) error {
	p := output.NewPrinter("")

	if contextName != "" {

		p.Running("Syncing knowledge context %q from hub…", contextName)
		return runKnowledgeImport(contextName, false, false)
	}

	docsDir := config.ResolveDocsDir(nil, loadProjectConfig())
	p.Running("Re-indexing knowledge wiki from %s/…", docsDir)
	return runKnowledgeIndex(docsDir, 0, false, false)
}

func runKnowledgeList() error {
	p := output.NewPrinter("")
	wikiDir := knowledge.WikiDir()
	entries, err := os.ReadDir(wikiDir)
	if err != nil {
		if os.IsNotExist(err) {
			p.Info("No knowledge wiki found. Run '%s knowledge index' first.", brand.BinName())
			return nil
		}
		return fmt.Errorf("list: %w", err)
	}
	var count int
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := strings.TrimSuffix(e.Name(), ".md")
		if name == "index" || name == "log" {
			continue
		}
		p.ListItem("%s", name)
		count++
	}
	p.Count("document", count)
	return nil
}

func runKnowledgeQuery(query string, contextName string) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	wikiDir := knowledge.WikiDir()
	if contextName != "" {
		wikiDir = knowledge.WikiDirForContext(contextName)
	}

	aiClient, err := ai.NewClientFromConfig()
	if err != nil {
		return fmt.Errorf("AI not configured: %w", err)
	}

	p.Running("Searching knowledge wiki…")
	result, err := wiki.SearchWiki(ctx, aiClient, query, wiki.SearchConfig{
		WikiDir:   wikiDir,
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

func runKnowledgeIndex(path string, workers int, reset, useLouvain bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	task := p.StartTask("Indexing %s into knowledge wiki...", path)

	wikiDir := knowledge.WikiDir()
	cfg := knowledge.IndexConfig{
		Workers:    workers,
		Reset:      reset,
		BatchSize:  100,
		UseLouvain: useLouvain,
	}

	result, err := knowledge.RunIndexPipeline(ctx, path, wikiDir, cfg)
	if err != nil {
		task.Fail("Indexing failed: %v", err)
		return fmt.Errorf("index failed: %w", err)
	}

	task.Done("Wiki generated: %d articles → %s (took %s)",
		result.IndexedFiles, wikiDir, result.TotalTime.Round(time.Millisecond))
	return nil
}

func runKnowledgeImport(name string, reset, useLouvain bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	gs, err := hub.NewGitStore(nil, loadProjectConfig())
	if err != nil {
		return fmt.Errorf("hub not configured — run '%s setup' first", brand.BinName())
	}

	branch := fmt.Sprintf("knowledge/project/%s", name)

	knowledge.EnsureContextCopy(name)

	wikiDir := knowledge.WikiDirForContext(name)
	globalCtxBase := filepath.Dir(wikiDir)
	localDocsDir := filepath.Join(globalCtxBase, "docs")

	task := p.StartTask("Importing knowledge context %q...", name)

	task.Update("Fetching knowledge from hub branch %s...", branch)
	if err := gs.ExtractBranchDir(branch, "docs", localDocsDir); err != nil {
		task.Fail("Fetch failed: %v", err)
		return fmt.Errorf("fetching knowledge from hub: %w", err)
	}
	p.Step("Docs synced → %s", localDocsDir)

	task.Update("Generating wiki...")
	cfg := knowledge.IndexConfig{
		Workers:    0,
		Reset:      reset,
		BatchSize:  100,
		UseLouvain: useLouvain,
	}
	result, err := knowledge.RunIndexPipeline(ctx, localDocsDir, wikiDir, cfg)
	if err != nil {
		task.Fail("Indexing failed: %v", err)
		return fmt.Errorf("indexing: %w", err)
	}
	task.Done("Wiki generated: %d articles", result.IndexedFiles)

	memStore, _ := memory.NewMemoryGitStore()
	wd, _ := os.Getwd()
	memory.OnHubImport(ctx, name, wd, memStore, nil)
	p.Step("Memory auto-cycle triggered for context %q (background)", name)

	p.Success("Context %q ready", name)
	p.Step("Query: %s knowledge query \"...\" --context %s", brand.BinName(), name)
	refreshModuleRule("knowledge", wd, "")
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

func runKnowledgeLint(contextName string, deep, fix bool, staleDays int) error {
	p := output.NewPrinter("")

	wikiDir := knowledge.WikiDir()
	if contextName != "" {
		wikiDir = knowledge.WikiDirForContext(contextName)
	}

	if _, err := os.Stat(wikiDir); err != nil {
		p.Info("No knowledge wiki found at %s. Run '%s knowledge index' first.", wikiDir, brand.BinName())
		return nil
	}

	p.Running("Auditing knowledge wiki…")

	cfg := wiki.LintConfig{
		Deep:      deep,
		Fix:       fix,
		StaleDays: staleDays,
	}

	report, err := wiki.LintWiki(wikiDir, cfg)
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

	if report.FixesApplied > 0 {
		p.Success("Applied %d fix(es)", report.FixesApplied)
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
		hash, err := memory.UserHashFromGit()
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

	ms, _ := memory.NewMemoryGitStore()

	svc := memory.NewMemoryService(scope, scopeID, ms)

	if err := svc.EnsureInitialised(); err != nil {

		_ = err
	}

	return svc, projectID, nil
}

func newMemorySvcForContext(contextName string) (*memory.MemoryService, error) {
	ms, err := memory.NewMemoryGitStore()
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
	p := output.NewPrinter("")

	scope := "project"
	if userScope {
		scope = "user"
	}

	dir := memory.RawDir(scope)
	if dir == "" {
		p.Info("No memories found in %s scope.", scope)
		return nil
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			p.Info("No memories found in %s scope.", scope)
			return nil
		}
		return err
	}

	termLower := strings.ToLower(term)
	var matches int
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		absPath := filepath.Join(dir, e.Name())
		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}
		content := strings.ToLower(string(data))
		if strings.Contains(content, termLower) {
			title, _ := memory.ParseMemoryMetaPublic(absPath)
			name := e.Name()
			var id string
			if memory.IsImportantMemory(name) {
				id = strings.TrimSuffix(name, memory.ImportantMemorySuffix+".md")
			} else {
				id = strings.TrimSuffix(name, ".md")
			}
			importantTag := ""
			if memory.IsImportantMemory(name) {
				importantTag = " ★"
			}
			p.ListItem("[%s] %s%s", id, title, importantTag)
			matches++
		}
	}

	if matches == 0 {
		p.Info("No memories matching %q in %s scope.", term, scope)
	} else {
		p.Count("match", matches)
	}
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

func runMemoryIndex(userScope bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	svc, _, err := newMemorySvc(userScope)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	task := p.StartTask("Indexing memories → %s...", svc.LocalDir())
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
	if err := svc.SyncToLocal(); err != nil {
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
	p.Success("Project memory wiki cleared. Raw memories are preserved in the git repository.")
	p.Step("Rebuild: %s memory index", brand.BinName())
	return nil
}

func runMemoryExport() error {
	p := output.NewPrinter("")
	ctx := context.Background()

	svc, _, err := newMemorySvc(false)
	if err != nil {
		return err
	}
	defer func() { _ = svc.Close() }()

	p.Running("Indexing and exporting project memories…")
	if err := svc.IndexMemories(ctx); err != nil {
		p.Warn("Wiki regeneration: %v", err)
	}

	if err := svc.SyncToLocal(); err != nil {
		p.Warn("Git sync: %v", err)
	}

	p.Success("Memories exported to git repository")
	p.Step("Other projects can import with: %s memory install <project-id>", brand.BinName())
	return nil
}

func runMemoryRemoveContext(contextName string) error {
	p := output.NewPrinter("")
	linkDir := filepath.Join(brand.DotDir(), "memory", contextName)
	if err := os.RemoveAll(linkDir); err != nil {
		return fmt.Errorf("removing context symlink: %w", err)
	}
	p.Success("Removed memory context %q", contextName)
	refreshModuleRule("memory", "", "")
	return nil
}

func runMemorySchema(contextName string) error {
	p := output.NewPrinter("")
	p.Header("Memory Graph Schema")
	p.Info("Node labels:  Document, Section")
	p.Info("Edge labels:  REFERENCES, CONTAINS")
	p.Info("Key properties:")
	p.Info("  Document: id (ULID), title, scope, scope_id, created_at, tags")
	p.Info("  Section:  name, summary, section_level")
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
	if err := svc.SyncToLocal(); err != nil {
		task.Fail("Sync failed: %v", err)
		return err
	}
	task.Done("Sync complete")
	return nil
}

func runMemoryWatch(rootPath string, useLouvain bool) error {
	p := output.NewPrinter("")
	p.Running("Watching %s for changes…", rootPath)
	return watchAndReindex(rootPath, useLouvain, func() error {
		return memory.RunProjectCycle(context.Background()).Err
	})
}

func runMemoryConsolidate(userScope, apply bool) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	scope := "project"
	if userScope {
		scope = "user"
	}

	aiClient, aiErr := ai.NewClientFromConfig()
	if aiErr != nil {
		p.Warn("AI not configured — only staleness detection will run (no duplicate/contradiction analysis)")
	}

	task := p.StartTask("Consolidating %s memories...", scope)
	report, err := memory.RunConsolidation(ctx, scope, aiClient)
	if err != nil {
		task.Fail("Consolidation failed: %v", err)
		return err
	}

	task.Done("Analyzed %d memories", report.TotalMemories)

	if !report.HasActions() {
		p.Success("No issues found — memory store is clean")
		return nil
	}

	if len(report.Duplicates) > 0 {
		p.Header("Duplicates (%d)", len(report.Duplicates))
		for _, a := range report.Duplicates {
			p.ListItem("%s", a.Reason)
		}
	}

	if len(report.Contradictions) > 0 {
		p.Header("Contradictions (%d)", len(report.Contradictions))
		for _, a := range report.Contradictions {
			p.ListItem("%s", a.Reason)
		}
	}

	if len(report.Stale) > 0 {
		p.Header("Stale (%d)", len(report.Stale))
		for _, a := range report.Stale {
			p.ListItem("[%s] %s — %s", a.MemoryIDs[0], a.Title, a.Reason)
		}
	}

	if len(report.Suggestions) > 0 {
		p.Header("Suggestions (%d)", len(report.Suggestions))
		for _, a := range report.Suggestions {
			p.ListItem("[%s] %s", a.Type, a.Reason)
		}
	}

	p.Blank()
	p.Step("Total: %d actions proposed", report.TotalActions())

	if apply {
		svc, _, svcErr := newMemorySvc(userScope)
		if svcErr != nil {
			return svcErr
		}
		defer func() { _ = svc.Close() }()

		applied := 0
		for _, a := range report.Suggestions {
			if len(a.MemoryIDs) == 0 {
				continue
			}
			id := a.MemoryIDs[0]
			switch a.Type {
			case "promote":
				if err := svc.PromoteMemory(id); err == nil {
					p.Step("Promoted: %s", id)
					applied++
				}
			case "demote":
				if err := svc.DemoteMemory(id); err == nil {
					p.Step("Demoted: %s", id)
					applied++
				}
			case "delete":
				if err := svc.RemoveMemory(id); err == nil {
					p.Step("Deleted: %s", id)
					applied++
				}
			}
		}
		if applied > 0 {
			p.Success("Applied %d action(s)", applied)
			refreshModuleRule("memory", "", "")
		} else {
			p.Info("No auto-applicable actions found (merges/updates require manual review)")
		}
	} else {
		p.Info("Run with --apply to execute safe suggestions (promote/demote/delete)")
	}

	return nil
}

func runMemoryGC(userScope, dryRun bool, staleDays int) error {
	p := output.NewPrinter("")
	ctx := context.Background()

	scope := "project"
	if userScope {
		scope = "user"
	}

	task := p.StartTask("Scanning %s memories for GC candidates (threshold: %d days)...", scope, staleDays)
	report, err := memory.RunGC(scope, staleDays)
	if err != nil {
		task.Fail("GC scan failed: %v", err)
		return err
	}

	task.Done("Scanned %d memories", report.TotalMemories)

	if len(report.Candidates) == 0 {
		p.Success("No GC candidates — memory store is healthy")
		return nil
	}

	p.Header("GC Candidates (%d)", len(report.Candidates))
	for _, c := range report.Candidates {
		ageStr := ""
		if c.Age > 0 {
			ageStr = fmt.Sprintf(" (%d days)", c.Age)
		}
		p.ListItem("[%s] %s%s — %s", c.ID, c.Title, ageStr, c.Reason)
	}

	if dryRun {
		p.Blank()
		p.Info("Dry run — no memories deleted. Use --dry-run=false to apply.")
		return nil
	}

	svc, _, svcErr := newMemorySvc(userScope)
	if svcErr != nil {
		return svcErr
	}
	defer func() { _ = svc.Close() }()

	deleted, _ := memory.ApplyGC(ctx, scope, report.Candidates, svc)
	p.Success("Deleted %d/%d candidates", deleted, len(report.Candidates))
	if deleted > 0 {
		refreshModuleRule("memory", "", "")
	}
	return nil
}

func watchAndReindex(rootPath string, useLouvain bool, reindex func() error) error {
	p := output.NewPrinter("")
	if err := reindex(); err != nil {
		p.Warn("Initial index error: %v", err)
	}

	g, err := git.DefaultErr()
	if err != nil {
		return fmt.Errorf("git required for watch: %w", err)
	}

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt, syscall.SIGTERM)

	var lastHash string
	pollTicker := time.NewTicker(2 * time.Second)
	defer pollTicker.Stop()

	for {
		select {
		case <-sig:
			output.Interrupted()
			return nil
		case <-pollTicker.C:
			hash := cliWatchHash(g, rootPath)
			if hash == lastHash || lastHash == "" {
				lastHash = hash
				continue
			}

			time.Sleep(500 * time.Millisecond)
			lastHash = cliWatchHash(g, rootPath)

			p.Running("Change detected, re-indexing…")
			if err := reindex(); err != nil {
				p.Warn("Re-index error: %v", err)
			} else {
				p.Success("Re-index complete")
			}
		}
	}
}

func cliWatchHash(g git.Git, dir string) string {
	status, _ := g.RunOutput(dir, "status", "--porcelain", "-uall")
	head, _ := g.RunOutput(dir, "rev-parse", "HEAD")

	mtimes := cliDirtyFileMtimes(status, dir)
	combined := head + "\n" + status + "\n" + mtimes
	h := sha256.Sum256([]byte(combined))
	return fmt.Sprintf("%x", h[:8])
}

func cliDirtyFileMtimes(porcelain, rootDir string) string {
	var b strings.Builder
	for _, line := range strings.Split(porcelain, "\n") {
		if len(line) < 4 {
			continue
		}
		rel := strings.TrimSpace(line[3:])
		if rel == "" || strings.HasSuffix(rel, "/") {
			continue
		}
		if info, err := os.Stat(filepath.Join(rootDir, rel)); err == nil {
			fmt.Fprintf(&b, "%s:%d\n", rel, info.ModTime().UnixNano())
		}
	}
	return b.String()
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
		CacheDir:         filepath.Dir(ictx.DBPath),
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
	linkDir := filepath.Join(brand.DotDir(), "knowledge", contextName)
	if err := os.RemoveAll(linkDir); err != nil {
		return fmt.Errorf("removing context symlink: %w", err)
	}
	p.Success("Removed knowledge context %q", contextName)
	refreshModuleRule("knowledge", "", "")
	return nil
}

func runKnowledgeExport() error {
	p := output.NewPrinter("")

	wd, _ := os.Getwd()
	gs, err := hub.NewGitStore(nil, loadProjectConfig())
	if err != nil {
		return fmt.Errorf("hub not configured — run '%s setup' first", brand.BinName())
	}

	lf, err := hub.LoadLockfile(lockfilePath())
	if err != nil || lf == nil {
		return fmt.Errorf("project not initialised — run '%s init' first", brand.BinName())
	}

	branch := fmt.Sprintf("knowledge/project/%s", lf.Project.ID)
	wt, err := gs.MemoryWorktree(branch)
	if err != nil {
		return fmt.Errorf("opening export branch: %w", err)
	}

	docsDir := config.ResolveDocsDir(nil, loadProjectConfig())
	docsSrc := filepath.Join(wd, docsDir)
	if _, err := os.Stat(docsSrc); err != nil {
		return fmt.Errorf("no %s/ directory found at %s", docsDir, wd)
	}

	p.Running("Exporting knowledge to hub branch %s…", branch)

	destDocs := filepath.Join(wt.Dir(), "docs")
	_ = os.RemoveAll(destDocs)
	if err := copyDirRecursive(docsSrc, destDocs); err != nil {
		return fmt.Errorf("copying %s: %w", docsDir, err)
	}
	p.Step("%s/ → %s", docsDir, destDocs)

	wikiDir := knowledge.WikiDir()
	if _, err := os.Stat(wikiDir); err == nil {
		destWiki := filepath.Join(wt.Dir(), "wiki")
		_ = os.RemoveAll(destWiki)
		if err := copyDirRecursive(wikiDir, destWiki); err != nil {
			p.Warn("Wiki copy: %v", err)
		} else {
			p.Step("wiki/ → %s", destWiki)
		}
	}

	if err := wt.CommitAndPush("export knowledge"); err != nil {
		return fmt.Errorf("pushing to hub: %w", err)
	}

	p.Success("Knowledge exported to branch %s", branch)
	p.Step("Other projects can import with: %s knowledge install %s", brand.BinName(), lf.Project.ID)
	return nil
}

func runKnowledgeWatch(rootPath string, useLouvain bool) error {
	p := output.NewPrinter("")
	p.Running("Watching %s for changes…", rootPath)
	return watchAndReindex(rootPath, useLouvain, func() error {
		ctx := context.Background()
		wikiDir := knowledge.WikiDir()
		cfg := knowledge.IndexConfig{UseLouvain: useLouvain}
		_, err := knowledge.RunIndexPipeline(ctx, rootPath, wikiDir, cfg)
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

func copyDirRecursive(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(src, path)
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
}

func runWikiSearch(query string, wikiRefs, hubRefs []string, sessionName string, continueSession bool, topK int) error {
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

		chatSources := make([]chat.WikiSource, len(wikiSources))
		for i, ws := range wikiSources {
			chatSources[i] = chat.WikiSource{
				ID:    ws.ID,
				Label: ws.Label,
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

	result, err := wiki.SearchMultiWiki(ctx, aiClient, query, wiki.MultiWikiSearchConfig{
		Sources:           wikiSources,
		UseBM25:           true,
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
		if len(s.WikiSources) > 0 {
			ids := make([]string, len(s.WikiSources))
			for i, ws := range s.WikiSources {
				ids[i] = ws.ID
			}
			p.Detail("Sources", strings.Join(ids, ", "))
		}
	}
	return nil
}
