package commands

import (
	"context"
	"fmt"
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newASTCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:               "ast",
		Short:             "Code knowledge graph — index, query, watch, and manage AST graphs",
		PersistentPreRunE: requireProject,
	}

	cmd.AddCommand(
		newASTIndexCmd(),
		newASTWatchCmd(),
		newASTQueryCmd(),
		newASTSchemaCmd(),
		newASTEmbedCmd(),
		newASTInstallCmd(),
		newASTRemoveCmd(),
		newASTSyncCmd(),
		newASTExportCmd(),
		newASTListCmd(),
		newASTSourceCmd(),
		newASTVerifyCmd(),
		newModuleRuleCmd("ast"),
	)

	return cmd
}

func newASTIndexCmd() *cobra.Command {
	var workers int
	var reset bool
	var reindex bool
	var cluster string
	var clusterPaths []string
	var noSource bool
	var grammar string

	cmd := &cobra.Command{
		Use:   "index [path...]",
		Short: "Parse source code and build the AST knowledge graph",
		Long: `Index source code into the knowledge graph.

Default mode: Tree-sitter auto-detection for all supported languages.

Cluster tagging:
  --cluster <name>         Tag all indexed nodes with a logical cluster name.
  --cluster-path <path=cluster>  Tag nodes under <path> with <cluster> (repeatable).
                     Paths are directory prefixes; most specific match wins.
                     Enables filtered queries: MATCH (n:Class {cluster: '<name>'}) RETURN n

Examples:
  ` + brand.BinName() + ` ast index
  ` + brand.BinName() + ` ast index ./src --cluster backend
  ` + brand.BinName() + ` ast index . --cluster my-module --reindex
  ` + brand.BinName() + ` ast index backend/ frontend/ shared/ --cluster-path backend/=python --cluster-path frontend/=javascript --cluster-path shared/=typescript
  ` + brand.BinName() + ` ast index backend/ frontend/ shared/ --cluster-path backend/=python --cluster-path frontend/=javascript --cluster-path shared/=typescript --cluster default

  # Then query by cluster:
  ` + brand.BinName() + ` ast query "MATCH (n:Function {cluster: 'backend'}) RETURN n.name, n.path LIMIT 20"
  ` + brand.BinName() + ` ast query "MATCH (n:Class {cluster: 'erp-core'}) RETURN n.name"
  ` + brand.BinName() + ` ast query "MATCH (n {cluster: 'my-module'}) RETURN label(n), n.name LIMIT 50"

Flags:
  --reindex   Re-parse every file, ignoring the parse cache, and rebuild the graph
              and the search index from the result. Keeps the cache directory, so
              embeddings of unchanged files are reused.
  --reset     Delete the whole store first — graph, search index AND caches — then
              index from scratch. Every embedding is recomputed.`,
		Args: cobra.ArbitraryArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			targetPaths := []string{"."}
			if len(args) > 0 {
				targetPaths = args
			}

			return runASTIndex(targetPaths, workers, reset, reindex, cluster, clusterPaths, noSource, grammar)
		},
	}
	cmd.Flags().IntVar(&workers, "workers", 0, "Parallel worker count (default: all CPUs)")
	cmd.Flags().BoolVar(&reset, "reset", false, "Delete the whole store first — graph, search index and caches — discarding every embedding")
	cmd.Flags().BoolVar(&reindex, "reindex", false, "Re-parse every file, ignoring the parse cache, keeping the cached embeddings")
	cmd.Flags().StringVar(&cluster, "cluster", "", "Tag all indexed nodes with a logical cluster name for filtered queries (fallback)")
	cmd.Flags().StringArrayVar(&clusterPaths, "cluster-path", nil, "Path-to-cluster mapping (repeatable), format: path=cluster (e.g., backend/=python)")
	cmd.Flags().BoolVar(&noSource, "no-source", false, "Skip storing source code in graph nodes (lighter index, no FTS/source retrieval)")
	cmd.Flags().StringVar(&grammar, "grammar", "", "Override grammar per extension (comma-separated: .ext=grammar-name, e.g. .sql=antlr-plsql,.pks=antlr-plsql)")
	return cmd
}

func newASTWatchCmd() *cobra.Command {
	var workers int
	var cluster string
	var clusterPaths []string

	cmd := &cobra.Command{
		Use:   "watch [path]",
		Short: "Watch source code for changes and re-index incrementally",
		Long: `Watch a directory for file changes and re-index modified files incrementally.

Default mode: Tree-sitter (best for incremental single-file re-parsing).

Cluster tagging:
  --cluster <name>         Tag all indexed nodes with a logical cluster name.
  --cluster-path <path=cluster>  Tag nodes under <path> with <cluster> (repeatable).
                     Paths are directory prefixes; most specific match wins.

Examples:
  ` + brand.BinName() + ` ast watch
  ` + brand.BinName() + ` ast watch --cluster my-cluster
  ` + brand.BinName() + ` ast watch --cluster-path backend/=python --cluster-path frontend/=javascript`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			targetPath := "."
			if len(args) > 0 {
				targetPath = args[0]
			}
			return runASTWatch(targetPath, workers, cluster, clusterPaths)
		},
	}
	cmd.Flags().IntVar(&workers, "workers", 0, "Parallel worker count (default: 2)")
	cmd.Flags().StringVar(&cluster, "cluster", "", "Tag all indexed nodes with a logical cluster name (fallback)")
	cmd.Flags().StringArrayVar(&clusterPaths, "cluster-path", nil, "Path-to-cluster mapping (repeatable), format: path=cluster (e.g., backend/=python)")
	return cmd
}

func newASTQueryCmd() *cobra.Command {
	var contextName string
	var aiMode bool
	var cypherOnly bool
	var aiOptimized bool
	var hybridMode bool

	var topK int

	cmd := &cobra.Command{
		Use:   "query <cypher-query | natural-language-question>",
		Short: "Query the AST knowledge graph (Cypher, natural language, or semantic search)",
		Long: `Execute a Cypher query, ask a natural language question, or perform hybrid search.

Without --ai or --hybrid: runs a raw Cypher query.
With --ai: generates Cypher from natural language via the configured AI provider.
With --hybrid: performs combined BM25 + semantic vector search via Reciprocal Rank Fusion (RRF).
  Combines keyword-based and meaning-based search for best results. Recommended default.
  Falls back to FTS-only when embeddings are unavailable.
With --cypher: prints the generated Cypher without executing it.
With --context: queries an imported context instead of the project graph.
With --ai-optimized: outputs results in a compact, token-efficient tabular format
  instead of verbose JSON. Reduces token consumption by 30-60%%. Recommended for
  AI agents and LLM pipelines.

Examples:
  ` + brand.BinName() + ` ast query "MATCH (f:Function) RETURN f.name LIMIT 10"
  ` + brand.BinName() + ` ast query "MATCH (f:Function) RETURN f.name, f.path LIMIT 10" --ai-optimized
  ` + brand.BinName() + ` ast query "show all functions that call validate_cpf" --ai
  ` + brand.BinName() + ` ast query "which tables are referenced?" --ai --cypher
  ` + brand.BinName() + ` ast query "procedures" --ai --context oracle-schema
  ` + brand.BinName() + ` ast query "authentication and login logic" --hybrid
  ` + brand.BinName() + ` ast query "authentication and login logic" --hybrid --top 20`,
		Args: cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			query := strings.Join(args, " ")
			if hybridMode {
				return runASTHybridSearch(query, contextName, topK, aiOptimized)
			}

			return runASTQuery(query, contextName, aiMode, cypherOnly, aiOptimized)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Query an imported context instead of the project")
	_ = cmd.RegisterFlagCompletionFunc("context", completionASTContexts())
	cmd.Flags().BoolVar(&aiMode, "ai", false, "Generate Cypher from natural language via AI")
	cmd.Flags().BoolVar(&cypherOnly, "cypher", false, "Print generated Cypher without executing (requires --ai)")
	cmd.Flags().BoolVar(&aiOptimized, "ai-optimized", false, "Output in compact, token-efficient tabular format (use --ai-optimized for TOON format)")
	cmd.Flags().BoolVar(&hybridMode, "hybrid", false, "Perform combined BM25 + semantic search via Reciprocal Rank Fusion (recommended)")
	cmd.Flags().IntVar(&topK, "top", 0, "Limit number of results for hybrid search (0 = no limit)")
	return cmd
}

func newASTSchemaCmd() *cobra.Command {
	var contextName string

	cmd := &cobra.Command{
		Use:   "schema",
		Short: "Show the AST graph schema and node properties",
		Long: `Print the comprehensive AST graph schema — node labels, properties, and relationships.

Useful for AI agents and LLMs to understand the graph structure before writing Cypher queries.

Examples:
  ` + brand.BinName() + ` ast schema
  ` + brand.BinName() + ` ast schema --context oracle-schema`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTSchema(contextName)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Show schema for an imported context")
	_ = cmd.RegisterFlagCompletionFunc("context", completionASTContexts())
	return cmd
}

func newASTVerifyCmd() *cobra.Command {
	var contextName string

	cmd := &cobra.Command{
		Use:   "verify",
		Short: "Check that the text in the graph is the text on disk",
		Long: `Compare every indexed node's text against the file it came from.

This exists for a failure nothing else here can see. LadybugDB's string corruption has
a SILENT form — a wrong (offset, length) landing on the valid text of another row — and
the value that comes back is well-formed, internally consistent, and simply wrong. The
usual scan finds invalid UTF-8 only, so it passes clean over a graph corrupted this way.

It DETECTS; it cannot repair. The defect is upstream, and sync will not fix it either:
the shard cache is keyed by content hash, so it reports the intact file as up to date
and never rewrites the row. A full reindex is what rewrites it.

Exit status is 1 when a divergence is found, so this can gate a pipeline.

Examples:
  ` + brand.BinName() + ` ast verify
  ` + brand.BinName() + ` ast verify --context oracle-schema`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTVerify(contextName)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Verify an imported context instead of this project")
	_ = cmd.RegisterFlagCompletionFunc("context", completionASTContexts())
	return cmd
}

func newASTInstallCmd() *cobra.Command {
	var contextName string
	var reset bool
	var list bool
	var workers int

	cmd := &cobra.Command{
		Use:   "install [path] --context <name>",
		Short: "Import an external project AST into a named context",
		Long: `Import an external project's source code into a separate AST database.

Each imported context gets its own icebug bundle at ~/` + brand.DotDir() + `/ast/<name>/graph.icebug,
mounted in-memory by its schema.cypher — no catalog file is written.

Examples:
  ` + brand.BinName() + ` ast install /path/to/project --context oracle-schema
  ` + brand.BinName() + ` ast install /path/to/project --context oracle-schema --reset
  ` + brand.BinName() + ` ast install --list`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if list {
				return runASTImportList()
			}
			if len(args) == 0 {
				return fmt.Errorf("path argument required (or use --list)")
			}
			if contextName == "" {
				return fmt.Errorf("--context is required for importing")
			}
			return runASTImport(args[0], contextName, reset, workers)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Name for the imported context (required)")
	cmd.Flags().BoolVar(&reset, "reset", false, "Wipe the context database before importing")
	cmd.Flags().BoolVar(&list, "list", false, "List all imported contexts")
	cmd.Flags().IntVar(&workers, "workers", 0, "Parallel worker count (default: all CPUs)")
	return cmd
}

func newASTRemoveCmd() *cobra.Command {
	var contextName string

	cmd := &cobra.Command{
		Use:   "remove",
		Short: "Remove the project AST graph or an imported context",
		Long: `Without --context: clears the project's AST database (source files kept).
With --context <name>: removes the imported context entirely (database + config).

Examples:
  ` + brand.BinName() + ` ast remove
  ` + brand.BinName() + ` ast remove --context oracle-schema`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTClean(contextName)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Remove an imported context by name")
	_ = cmd.RegisterFlagCompletionFunc("context", completionASTContexts())
	return cmd
}

func newASTSyncCmd() *cobra.Command {
	var contextName string

	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Re-sync an imported AST context from the global cache",
		Long: `Re-installs files for an imported AST context from the global cache.
Use --context to specify which context to sync.

Examples:
  ` + brand.BinName() + ` ast sync --context oracle-schema`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTSync(contextName)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Sync a specific imported context by name")
	_ = cmd.RegisterFlagCompletionFunc("context", completionASTContexts())
	return cmd
}

func newASTExportCmd() *cobra.Command {
	var format string
	var outputDir string
	var noSources bool

	cmd := &cobra.Command{
		Use:   "export",
		Short: "Export the AST knowledge graph (Obsidian vault or .ast bundle)",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTExport(format, outputDir, noSources)
		},
	}
	cmd.Flags().StringVar(&format, "format", "obsidian", "Export format (obsidian, bundle)")
	cmd.Flags().StringVar(&outputDir, "output", brand.ProjectRuntimePath(".", "ast", "export"), "Output path")
	cmd.Flags().BoolVar(&noSources, "no-sources", false, "Exclude file source content from bundle export")
	return cmd
}

func newASTListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all installed AST contexts (including the local project)",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTImportList()
		},
	}
}

func newASTSourceCmd() *cobra.Command {
	var contextName string
	var entity string
	var entityType string
	var head int
	var tail int
	var startLine int
	var endLine int
	var pattern string
	var isRegex bool
	var before int
	var after int
	var lineNumbers bool

	cmd := &cobra.Command{
		Use:   "source <relative-path>",
		Short: "Show the stored source code for a file with grep/head/tail capabilities",
		Long: `Retrieve and display stored source content from the code graph with IDE-like capabilities.

The path should be relative to the project root (as stored in the graph).

Options allow viewing portions of files, searching for patterns, and extracting
entity source code using line range information from the graph.

Examples:
  ` + brand.BinName() + ` ast source internal/auth/jwt.go
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --head 20
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --tail 30
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --start 50 --end 80
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --entity ValidateToken
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --entity ValidateToken --entity-type Function
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --pattern "func.*Validate" --regex --before 2 --after 5
  ` + brand.BinName() + ` ast source internal/auth/jwt.go --line-numbers --head 10
  ` + brand.BinName() + ` ast source pkg/forms.go --context oracle-schema`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runASTSource(args[0], contextName, entity, entityType, head, tail, startLine, endLine, pattern, isRegex, before, after, lineNumbers)
		},
	}
	cmd.Flags().StringVar(&contextName, "context", "", "Look up source in an imported context")
	_ = cmd.RegisterFlagCompletionFunc("context", completionASTContexts())
	cmd.Flags().StringVar(&entity, "entity", "", "Entity name (function, class, etc.) to extract using line range from graph")
	cmd.Flags().StringVar(&entityType, "entity-type", "", "Entity type for disambiguation: Function, Class, Method, Struct, etc.")
	cmd.Flags().IntVar(&head, "head", 0, "Show only the first N lines")
	cmd.Flags().IntVar(&tail, "tail", 0, "Show only the last N lines")
	cmd.Flags().IntVar(&startLine, "start", 0, "Start line number (1-indexed)")
	cmd.Flags().IntVar(&endLine, "end", 0, "End line number (1-indexed, inclusive)")
	cmd.Flags().StringVar(&pattern, "pattern", "", "Search for a pattern (literal text or regex with --regex)")
	cmd.Flags().BoolVar(&isRegex, "regex", false, "Treat --pattern as a regular expression")
	cmd.Flags().IntVar(&before, "before", 0, "Number of context lines before each pattern match")
	cmd.Flags().IntVar(&after, "after", 0, "Number of context lines after each pattern match")
	cmd.Flags().BoolVar(&lineNumbers, "line-numbers", false, "Include line numbers in the output")
	return cmd
}

func newASTEmbedCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "embed",
		Short: "Generate vector embeddings for semantic search",
		Long: `Compute vector embeddings for all code entities in the AST graph.

Embeddings enable semantic search (` + brand.BinName() + ` ast query --semantic).
Only entities that are new or have changed since the last run are processed,
making subsequent runs fast when nothing has changed.

Requires an embedding provider to be configured (see ` + brand.BinName() + ` setup).`,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			ctx := context.Background()

			task := p.StartTask("Checking pending embeddings...")

			cfg := ast.DefaultEmbeddingConfig()
			cfg.RepoRoot, _ = os.Getwd()
			cfg.ProjectDir = cfg.RepoRoot
			ladybugCfg := ast.DefaultLadybugConfig()
			cacheDir := ladybugCfg.StoreDir

			parseCache, cacheErr := ast.NewShardCache(cacheDir)
			if cacheErr != nil {
				task.Fail("Parse cache: %v", cacheErr)
				return nil
			}
			parseCache.SetRoot(cfg.RepoRoot)
			cfg.ParseCache = parseCache

			idxPath := ladybugCfg.StoreDir
			searchIdx, idxErr := ast.OpenSearchIndex(ctx, idxPath)
			if idxErr != nil {
				task.Fail("Search index: %v", idxErr)
				return nil
			}
			defer func() { _ = searchIdx.Close() }()
			cfg.Index = searchIdx

			if embCache, embErr := ast.NewShardEmbCache(cacheDir, parseCache); embErr == nil {
				cfg.EmbCache = embCache
				defer func() { _ = embCache.Close() }()
			}

			probe := ast.NewEmbedder(nil, cfg)
			pending := probe.CountPending(ctx)
			if pending == 0 {
				// Even if all embeddings are cached, the search tables may be empty —
				// a graph rebuilt without them, or a store restored from a partial
				// build. Rebuild from cache so search works without re-embedding.
				// Emptiness has to be queried now that the index is not a file of its
				// own; a stat only proves the graph is there.
				if !ast.SearchIndexBuilt(ctx, idxPath) {
					task.Update("Rebuilding search index...")
					if rbErr := searchIdx.RebuildFromCache(ctx, parseCache, ast.BuildEmbLookup(parseCache, cfg.EmbCache)); rbErr != nil {
						p.StepWarn("Search index rebuild: %v", rbErr)
					}
					task.Done("Search index rebuilt")
				} else {
					task.Done("All entities up to date")
				}
				return nil
			}

			task.Update("Loading embedding model...")
			embClient, err := ai.NewEmbeddingClientFromConfig()
			if err != nil {
				task.Fail("Embedding client: %v", err)
				return nil
			}

			cfg.OnProgress = func(done, total int) {
				task.Update("Embedding: %d / %d", done, total)
			}
			embedder := ast.NewEmbedder(embClient, cfg)
			n, err := embedder.RunCycle(ctx)
			if err != nil {
				task.Fail("Embedding cycle: %v", err)
				return nil
			}

			task.Done("%d entities embedded", n)
			return nil
		},
	}
}
