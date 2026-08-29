package mcpstdio

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type knowledgeIndexInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory to index (required)"`
	Path        string `json:"path,omitempty" jsonschema:"Index this directory wholesale instead of the configured scope. Omit to index knowledge.docs_dir (default docs/) plus the root README."`
	Workers     int    `json:"workers,omitempty" jsonschema:"Number of parallel worker threads"`
	Reset       bool   `json:"reset,omitempty" jsonschema:"Clear graph and re-index from scratch"`
	UseLouvain  bool   `json:"louvain,omitempty" jsonschema:"Use Louvain community detection"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeSearchInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query       string `json:"query" jsonschema:"Keywords to search for in the knowledge wiki using BM25"`
	TopK        int    `json:"top_k,omitempty" jsonschema:"Maximum number of results (0 = no limit)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context to search"`
	Preview     *bool  `json:"preview,omitempty" jsonschema:"Set to true to include a short text excerpt per hit. Default false: a search answers with titles, and the page is read with wiki_source when the agent decides it needs it"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeSchemaInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Named imported context"`
}

type knowledgeLintInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Deep        bool   `json:"deep,omitempty" jsonschema:"Enable AI-assisted contradiction detection"`
	Fix         bool   `json:"fix,omitempty" jsonschema:"Auto-repair fixable issues (backlinks)"`
	StaleDays   int    `json:"stale_days,omitempty" jsonschema:"Mark pages older than N days as stale"`
	Context     string `json:"context,omitempty" jsonschema:"Lint an imported context by name"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type knowledgeInstallInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Name       string `json:"name" jsonschema:"Name of the knowledge context to import from the hub"`
}

type knowledgeRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Name of the imported context to remove. If empty, clears local project knowledge wiki."`
}

type knowledgeSyncInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Context    string `json:"context,omitempty" jsonschema:"Sync a specific imported context by name. If empty, syncs local docs/ index."`
}

type knowledgeExportInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
}

type knowledgeListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

func registerKnowledgeTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "index"),
		Description: "Index the documentation tree (knowledge.docs_dir, default docs/) plus the project's root README into the knowledge graph and regenerate the wiki. Pass path to index a specific directory wholesale instead.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeIndexInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)

		// No path: the configured docs tree plus the root README, scoped under the
		// project so every reported source path resolves from the project root. An
		// explicit path is taken literally and indexed wholesale.
		root := projectDir
		scope := knowledge.ScopeFor(projectDir, nil, projectCfg)
		if input.Path != "" {
			root = input.Path
			if !filepath.IsAbs(root) {
				root = filepath.Join(projectDir, root)
			}
			scope = knowledge.WikiScope{}
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		cfg := knowledge.IndexConfig{
			Workers:    input.Workers,
			Reset:      input.Reset,
			BatchSize:  100,
			UseLouvain: input.UseLouvain,
			ProjectCfg: projectCfg,
			Scope:      scope,
		}

		var result *knowledge.IndexResult
		err = withProjectDir(projectDir, func() error {
			var ierr error
			result, ierr = knowledge.RunIndexPipeline(ctx, root, wikiDir, cfg)
			return ierr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(result)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("knowledge", "search"),
		Description: "Search the project knowledge wiki using BM25 keyword ranking. " +
			"Answers with page titles and scores, not page text: pick the page from the titles, then read it with " +
			brand.MCPToolName("wiki", "source") + ", which slices. Pass preview=true only when the titles are not enough to choose.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(noKnowledgeToSearch(projectDir, input.Context))
		}

		var results []wiki.BM25Result
		err = withProjectDir(projectDir, func() error {
			results = wiki.BM25Search(ctx, wikiDir, input.Query, input.TopK)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return textResult(wiki.FormatBM25ResultsTOON(results, wantPreview(input.Preview)))
		}
		return jsonResult(results)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "schema"),
		Description: "Show the knowledge graph schema and node properties.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSchemaInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		schemaText := fmt.Sprintf("KNOWLEDGE Wiki\nWiki directory: %s\nArchitecture: file-based wiki (no graph database)", wikiDir)
		return textResult(schemaText)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "lint"),
		Description: "Audit the knowledge wiki for structural issues.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeLintInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		wikiDir := resolveWikiDir("knowledge", projectDir, input.Context)
		if wikiDir == "" {
			return errResult(fmt.Errorf("knowledge wiki not found"))
		}

		cfg := wiki.LintConfig{
			Deep:      input.Deep,
			Fix:       input.Fix,
			StaleDays: input.StaleDays,
		}
		if cfg.StaleDays <= 0 {
			cfg.StaleDays = 30
		}

		var report *wiki.LintReport
		err = withProjectDir(projectDir, func() error {
			var lerr error
			report, lerr = wiki.LintWiki(wikiDir, cfg)
			return lerr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(report)
		}
		return jsonResult(report)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "install"),
		Description: "Import an external knowledge context from the hub.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeInstallInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		st, err := hub.NewS3Store(ctx, nil, projectCfg)
		if err != nil || !st.Configured() {
			return errResult(fmt.Errorf("hub not configured: %w", err))
		}

		var chunks int
		err = withProjectDir(projectDir, func() error {
			n, ierr := installKnowledgeContext(ctx, st, input.Name)
			if ierr != nil {
				return ierr
			}
			chunks = n
			if err := store.AddContext(projectDir, store.KindKnowledge, store.ContextRecord{
				Name: input.Name,
			}); err != nil {
				return fmt.Errorf("registering knowledge context: %w", err)
			}
			// The branch pair is keyed by the same project id, so importing a
			// project's documentation also brings the memories that explain it.
			memStore, _ := memory.NewMemoryStore()
			memory.OnHubImport(ctx, input.Name, projectDir, memStore, nil)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if chunks == 0 {
			return textResult(fmt.Sprintf(
				"Knowledge context %q imported, but it published no compiled wiki — "+
					"nothing is searchable. Ask its publisher to run knowledge export.", input.Name))
		}
		return textResult(fmt.Sprintf("Knowledge context %q imported: %d chunks indexed.", input.Name, chunks))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "remove"),
		Description: "Remove the project knowledge graph or an imported context.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Context != "" {
			cleanCtx, err := sanitizeContextName(input.Context)
			if err != nil {
				return errResult(err)
			}
			// Only this project's claim is dropped. The wiki is global and another
			// project may have imported the same context.
			if err := store.RemoveContext(projectDir, store.KindKnowledge, cleanCtx); err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Knowledge context %q removed.", cleanCtx))
		}

		wikiDir := knowledge.WikiDirFor(projectDir)
		if err := os.RemoveAll(wikiDir); err != nil {
			return errResult(err)
		}
		_ = os.MkdirAll(wikiDir, 0o755)
		return textResult("Project knowledge wiki cleared.")
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "sync"),
		Description: "Re-sync an imported context from the global cache or rebuild local wiki.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeSyncInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Context != "" {
			cleanCtx, err := sanitizeContextName(input.Context)
			if err != nil {
				return errResult(err)
			}
			projectCfg := loadProjectConfig(projectDir)
			st, err := hub.NewS3Store(ctx, nil, projectCfg)
			if err != nil || !st.Configured() {
				return errResult(err)
			}
			var chunks int
			err = withProjectDir(projectDir, func() error {
				n, ierr := installKnowledgeContext(ctx, st, cleanCtx)
				chunks = n
				return ierr
			})
			if err != nil {
				return errResult(err)
			}
			return textResult(fmt.Sprintf("Sync context %q complete: %d chunks indexed.", cleanCtx, chunks))
		}

		projectCfg := loadProjectConfig(projectDir)
		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		cfg := knowledge.IndexConfig{
			Workers:    4,
			ProjectCfg: projectCfg,
			Scope:      knowledge.ScopeFor(projectDir, nil, projectCfg),
		}
		var result *knowledge.IndexResult
		err = withProjectDir(projectDir, func() error {
			var ierr error
			result, ierr = knowledge.RunIndexPipeline(ctx, projectDir, wikiDir, cfg)
			return ierr
		})
		if err != nil {
			return errResult(err)
		}
		return jsonResult(result)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "export"),
		Description: "Export the project knowledge wiki and graph to the hub.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeExportInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		projectCfg := loadProjectConfig(projectDir)
		st, err := hub.NewS3Store(ctx, nil, projectCfg)
		if err != nil || !st.Configured() {
			return errResult(err)
		}

		lf, err := hub.LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
		if err != nil || lf == nil {
			return errResult(fmt.Errorf("project not initialised"))
		}

		prefix := hub.ContextPrefix("knowledge", lf.Project.ID)

		wikiDir := resolveWikiDir("knowledge", projectDir, "")
		if _, statErr := os.Stat(wikiDir); statErr != nil {
			return errResult(fmt.Errorf(
				"no compiled wiki at %s — run graphit_knowledge_index first, "+
					"since what is published is the compiled wiki and not the docs tree", wikiDir))
		}

		// The documentation TREE is deliberately not published. It lives in this
		// project's own repository, the consumer never compiles it, and shipping it
		// meant every consumer paid for the embedding model again over text whose
		// vectors were in the shards beside it.
		staged, err := os.MkdirTemp("", brand.TempDirPrefix("knowledge-export"))
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = os.RemoveAll(staged) }()

		if err := exportWikiToWorktree(wikiDir, staged); err != nil {
			return errResult(err)
		}
		if err := st.PublishContextDir(ctx, "knowledge", lf.Project.ID, "wiki", staged); err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Knowledge exported to %s", prefix))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("knowledge", "list"),
		Description: "List all articles in the local knowledge wiki.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input knowledgeListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var articles []string
		err = withProjectDir(projectDir, func() error {
			wikiDir := resolveWikiDir("knowledge", projectDir, "")
			if wikiDir == "" {
				return noKnowledgeToSearch(projectDir, "")
			}
			entries, err := os.ReadDir(wikiDir)
			if err != nil {
				return err
			}
			for _, e := range entries {
				if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
					continue
				}
				name := strings.TrimSuffix(e.Name(), ".md")
				if name == "index" || name == "log" {
					continue
				}
				articles = append(articles, name)
			}
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(articles)
		}
		return jsonResult(articles)
	}))
}

// noKnowledgeToSearch explains an empty source set, which has three different causes
// and only one of them is a mistake.
func noKnowledgeToSearch(projectDir, contextName string) error {
	if contextName != "" {
		installed := knowledge.InstalledContextsIn(projectDir)
		if len(installed) == 0 {
			return fmt.Errorf("no knowledge context named %q — this project has none installed", contextName)
		}
		return fmt.Errorf("no knowledge context named %q — installed here: %s",
			contextName, strings.Join(installed, ", "))
	}
	if store.IsEphemeralProject(projectDir) {
		installed := knowledge.InstalledContextsIn(projectDir)
		if len(installed) == 0 {
			return fmt.Errorf("this live search session selected no documentation set, so there is nothing to search")
		}
		return fmt.Errorf("a live search session has no documentation wiki of its own: the sets it can search are %s — pass one as 'context'",
			strings.Join(installed, ", "))
	}
	return fmt.Errorf("knowledge wiki not found")
}

// installKnowledgeContext replaces a context's stored wiki with what its branch
// carries and builds the search index from the shards that arrived.
//
// A context publishes the COMPILED wiki, so there is no generator in this path: the consumer
// indexes what it was given. A context with no `wiki` prefix fetches nothing and indexes zero
// chunks, which the caller reports rather than hides.
func installKnowledgeContext(ctx context.Context, st *hub.S3Store, name string) (int, error) {
	dir, err := knowledge.ResetContextWiki(name)
	if err != nil {
		return 0, err
	}
	if err := st.FetchContextDir(ctx, "knowledge", name, "wiki", dir); err != nil {
		return 0, fmt.Errorf("fetching %s from hub: %w", hub.ContextPrefix("knowledge", name), err)
	}
	return knowledge.IndexContextWiki(ctx, name)
}

// exportWikiToWorktree mirrors a compiled wiki into a hub worktree, leaving the
// database behind.
//
// wiki.db is derived and it is the largest thing in the directory; the shards beside
// it are what a consumer actually needs, and it rebuilds the database from them in
// seconds. Mirroring rather than copying is what makes a deleted page disappear from
// the branch instead of outliving its source.
func exportWikiToWorktree(wikiDir, worktreeDir string) error {
	dest := filepath.Join(worktreeDir, "wiki")
	if err := paths.SyncCopyDirExcept(wikiDir, dest, wiki.IsDerivedFile); err != nil {
		return fmt.Errorf("publishing wiki %s: %w", wikiDir, err)
	}
	return nil
}

func copyDirRecursive(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
}
