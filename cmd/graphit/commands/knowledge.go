package commands

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/lancequery"
	"github.com/spf13/cobra"
)

func newKnowledgeCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "knowledge",
		Aliases: []string{"kn"},
		Short:   "Knowledge wiki — index, query, and inspect knowledge contexts.",
		Long: brand.DisplayName + ` Knowledge — LLM wiki generated from docs/.

Indexes docs/ into a navigable knowledge wiki. Versioned external contexts are
installed through the Hub and queried here.

Commands:
  index    Index the project docs/ into the knowledge index and wiki
  export   Export an importable package, OKF, or Obsidian vault
  schema   Show the index tables, their columns and row counts
  query    Filter rows of one index table and project columns
  search   Rank pages by relevance with BM25
  ask      Have the configured AI answer a question from the wiki
  remove   Remove the project knowledge index or an imported context
  sync     Rebuild the local project wiki
  list     List all installed knowledge contexts
  rule     Customize the global knowledge agent rule

Examples:
  ` + brand.BinName() + ` knowledge index --louvain
  ` + brand.BinName() + ` knowledge export --format package
  ` + brand.BinName() + ` knowledge query --filter "stale_since != ''"
  ` + brand.BinName() + ` knowledge ask "how does auth work?"
  ` + brand.BinName() + ` hub install team-platform --type knowledge
  ` + brand.BinName() + ` knowledge remove --context team-platform
  ` + brand.BinName() + ` knowledge list`,
	}

	cmd.AddCommand(
		newKnowledgeIndexCmd(),
		newKnowledgeExportCmd(),
		newKnowledgeWatchCmd(),
		newKnowledgeTableQueryCmd(),
		newKnowledgeAskCmd(),
		newKnowledgeSearchCmd(),
		newKnowledgeLintCmd(),
		newKnowledgeSchemaCmd(),
		newKnowledgeRemoveCmd(),
		newKnowledgeSyncCmd(),
		newKnowledgeListCmd(),
		newModuleRuleCmd("knowledge"),
	)

	return cmd
}

func newKnowledgeExportCmd() *cobra.Command {
	var (
		format      string
		contextName string
		projectDir  string
		outputPath  string
	)
	cmd := &cobra.Command{
		Use:   "export",
		Short: "Export Knowledge as an importable package, OKF, or Obsidian vault",
		Long: `Export the compiled Knowledge index in one of three formats:

  package   Native, queryable .knowledge package accepted by Hub Upload
  okf       Open Knowledge Format Markdown directory
  obsidian  Navigable Obsidian Markdown vault

Examples:
  ` + brand.BinName() + ` knowledge export --format package
  ` + brand.BinName() + ` knowledge export --format okf --output ./knowledge-okf
  ` + brand.BinName() + ` knowledge export --format obsidian --output ./knowledge-vault`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeExport(format, contextName, projectDir, outputPath)
		},
	}
	cmd.Flags().StringVar(&format, "format", "okf", "Export format (package, okf, obsidian)")
	cmd.Flags().StringVar(&contextName, "context", "", "Imported knowledge context name")
	cmd.Flags().StringVar(&projectDir, "project-dir", "", "Project directory (defaults to the working directory)")
	cmd.Flags().StringVar(&outputPath, "output", "", "Output file or directory (format-specific default under the project runtime directory)")
	return cmd
}

func newKnowledgeIndexCmd() *cobra.Command {
	var (
		reset      bool
		useLouvain bool
		workers    int
	)
	cmd := &cobra.Command{
		Use:   "index [path]",
		Short: "Index the docs tree into the knowledge index and regenerate the wiki",
		Long: `Scan the documentation tree and build a persistent knowledge wiki.

Without a path, this indexes knowledge.docs_dir (default: docs/) plus the
project's root README. Override the tree with --config knowledge.docs_dir=<dir>,
and drop the README with --config knowledge.include_readme=false.

Passing a path indexes that directory wholesale instead, README rule included or
not — it is an explicit request, so it is taken literally.

Project flags (--reset, --louvain) apply to the local project index.

Examples:
  ` + brand.BinName() + ` knowledge index
  ` + brand.BinName() + ` knowledge index --reset --louvain
  ` + brand.BinName() + ` knowledge index documentation/`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				return runKnowledgeIndex(args[0], knowledge.WikiScope{}, workers, reset, useLouvain)
			}
			wd, err := os.Getwd()
			if err != nil {
				return err
			}
			scope := knowledge.ScopeFor(wd, parseInlineConfig(cmd), loadProjectConfig())
			return runKnowledgeIndex(wd, scope, workers, reset, useLouvain)
		},
	}
	cmd.Flags().BoolVar(&reset, "reset", false, "Clear graph and re-index from scratch (project only)")
	cmd.Flags().BoolVar(&useLouvain, "louvain", false, "Use Louvain community detection (project only)")
	cmd.Flags().IntVar(&workers, "workers", 0, "Parallel workers (0 = sequential)")
	return cmd
}

func newKnowledgeAskCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "ask <text>",
		Short: "Answer a question from the knowledge wiki using AI",
		Long: `Search the knowledge wiki using the AI consultation cycle.

The wiki module presents index.md to the AI, then cycles through page
requests until the AI has enough context to answer it. Only the generated
wiki is used; the index itself is reached with ` + brand.BinName() + ` knowledge query.

For a structured question answered from the index itself, without an AI, use
` + brand.BinName() + ` knowledge query.

With --context: searches an imported context instead of the project wiki.

Examples:
  ` + brand.BinName() + ` knowledge ask "how does authentication work?"
  ` + brand.BinName() + ` knowledge ask "auth patterns" --context team-platform`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeQuery(args[0], context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Query an imported context by name")
	return cmd
}

func newKnowledgeSearchCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "search <query>",
		Short: "Search the knowledge wiki using BM25 keyword ranking",
		Long: `Search the knowledge wiki using FTS5 + BM25 keyword ranking.

Returns ranked results without AI — fast, local, and deterministic.
Use 'ask' for AI-powered deep consultation, and 'query' to filter index rows
by predicate when you already know what you are looking for.

With --context: searches an imported context instead of the project wiki.

Examples:
  ` + brand.BinName() + ` knowledge search "authentication"
  ` + brand.BinName() + ` knowledge search "auth" --context team-platform`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeSearch(args[0], context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Search an imported context by name")
	return cmd
}

func newKnowledgeRemoveCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "remove",
		Short: "Remove the project knowledge index or an imported context",
		Long: `Without --context: deletes the project wiki index directory (source docs kept).
With --context <name>: removes the named imported context from this project.

Examples:
  ` + brand.BinName() + ` knowledge remove
  ` + brand.BinName() + ` knowledge remove --context team-platform`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if context != "" {
				return runKnowledgeRemoveContext(context)
			}
			return runKnowledgeClean()
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Name of the imported context to remove")
	return cmd
}

func newKnowledgeSyncCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Rebuild the local project knowledge wiki",
		Long: `Re-index the configured documentation scope into the local project wiki.

Examples:
	  ` + brand.BinName() + ` knowledge sync`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeSync()
		},
	}
	return cmd
}

func newKnowledgeWatchCmd() *cobra.Command {
	var useLouvain bool
	cmd := &cobra.Command{
		Use:   "watch [path]",
		Short: "Watch the docs tree for changes and re-index + regenerate wiki incrementally",
		Long: `Watch for file changes and incrementally re-index modified files, then
regenerate the knowledge wiki. Delegates to the wiki engine's watch mode.

Without a path, this watches the project and rebuilds from knowledge.docs_dir
(default: docs/) plus the root README. Passing a path watches and indexes that
directory wholesale.

Only project scope is supported for watch (not imported contexts).

Examples:
  ` + brand.BinName() + ` knowledge watch
  ` + brand.BinName() + ` knowledge watch documentation/ --louvain`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				return runKnowledgeWatch(args[0], knowledge.WikiScope{}, useLouvain)
			}
			wd, err := os.Getwd()
			if err != nil {
				return err
			}
			scope := knowledge.ScopeFor(wd, parseInlineConfig(cmd), loadProjectConfig())
			return runKnowledgeWatch(wd, scope, useLouvain)
		},
	}
	cmd.Flags().BoolVar(&useLouvain, "louvain", false, "Use Louvain community detection on wiki regeneration")
	return cmd
}

func newKnowledgeSchemaCmd() *cobra.Command {
	var context, table string
	cmd := &cobra.Command{
		Use:   "schema",
		Short: "Show the knowledge index tables, their columns and row counts",
		Long: `Print the shape of the knowledge index: every table, every column with its type, and
how many rows each holds. Read this before writing a ` + brand.BinName() + ` knowledge query filter.

The index is LanceDB, not a graph database. Pages live in chunks; the links between
them live in xrefs; sync_log holds the index history and meta its own metadata. A
page's body, summary and search terms are marked heavy — left out of a default
projection for size — and the embedding vector is never returned as numbers.

Examples:
  ` + brand.BinName() + ` knowledge schema
  ` + brand.BinName() + ` knowledge schema --table xrefs
  ` + brand.BinName() + ` knowledge schema --context team-platform`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeSchema(cmd.Context(), context, table)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Show schema for an imported context")
	cmd.Flags().StringVar(&table, "table", "", "Describe only this table")
	return cmd
}

func newKnowledgeTableQueryCmd() *cobra.Command {
	var (
		context string
		table   string
		filter  string
		columns []string
		limit   int
		offset  int
	)
	cmd := &cobra.Command{
		Use:   "query",
		Short: "Filter rows of one knowledge index table and return only the columns asked for",
		Long: `Ask a structured question about the knowledge index.

--filter is a Lance SQL predicate: a WHERE clause over the table's own columns. It is
not SQL — there is no SELECT, JOIN, GROUP BY or aggregate, and the engine offers no
ORDER BY, so rows come back in storage order.

This answers questions a ranked search cannot: which pages are stale, what links to a
given page. Use ` + brand.BinName() + ` knowledge search to find pages by relevance, ` + brand.BinName() + ` wiki source to
read one, and ` + brand.BinName() + ` knowledge ask to have an AI answer from them.

Examples:
  ` + brand.BinName() + ` knowledge query --filter "stale_since != ''" --columns slug,title,stale_reason
  ` + brand.BinName() + ` knowledge query --table xrefs --filter "target_slug = 'storage-layout'"
  ` + brand.BinName() + ` knowledge query --filter "doc_type = 'guide'" --columns slug,title`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeTableQuery(cmd.Context(), context, lancequery.Request{
				Table: table, Filter: filter, Columns: columns, Limit: limit, Offset: offset,
			})
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Query an imported context by name")
	cmd.Flags().StringVar(&table, "table", "chunks", "Table to query; knowledge schema lists them")
	cmd.Flags().StringVar(&filter, "filter", "", "Lance SQL predicate; empty matches every row")
	cmd.Flags().StringSliceVar(&columns, "columns", nil, "Columns to return; empty returns every compact column")
	cmd.Flags().IntVar(&limit, "limit", 0, "Rows to return (default 20); unlike the MCP tool this has no ceiling")
	cmd.Flags().IntVar(&offset, "offset", 0, "Rows to skip")
	return cmd
}

func newKnowledgeListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all installed knowledge contexts (including the local project)",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeList()
		},
	}
}

func newKnowledgeLintCmd() *cobra.Command {
	var (
		staleDays int
		context   string
	)
	cmd := &cobra.Command{
		Use:   "lint",
		Short: "Audit the knowledge wiki for structural issues",
		Long: `Run a comprehensive audit of the knowledge wiki:

  • Orphan pages: entities with no inbound or outbound wikilinks
  • Broken links: [[wikilinks]] pointing to non-existent pages
  • Stale pages: entities not updated within --stale-days
  • Empty pages: entities with minimal content (≤ 10 words)
  • Missing frontmatter: required YAML fields (title, tags, updated)

Examples:
  ` + brand.BinName() + ` knowledge lint
  ` + brand.BinName() + ` knowledge lint --stale-days 7`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeLint(context, staleDays)
		},
	}
	cmd.Flags().IntVar(&staleDays, "stale-days", 30, "Mark pages older than N days as stale")
	cmd.Flags().StringVar(&context, "context", "", "Lint an imported context by name")
	return cmd
}
