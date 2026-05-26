package commands

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/spf13/cobra"
)

func newKnowledgeCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "knowledge",
		Aliases: []string{"kn"},
		Short:   "Knowledge wiki — index, query, and manage knowledge contexts.",
		Long: brand.DisplayName + ` Knowledge — LLM wiki generated from docs/.

Indexes docs/ into a navigable knowledge graph wiki. Supports importing
external knowledge contexts from the hub and querying with natural language or Cypher.

Commands:
  index    Index the project docs/ into the knowledge graph and wiki
  query    Query the knowledge graph (Cypher or AI natural language)
  install  Import an external knowledge context from the hub
  remove   Remove the project knowledge graph or an imported context
  sync     Re-sync an imported context from the global cache
  export   Export the project wiki and graph to the hub
  list     List all installed knowledge contexts
  rule     Customize the global knowledge agent rule

Examples:
  ` + brand.BinName() + ` knowledge index --louvain
  ` + brand.BinName() + ` knowledge query "how does auth work?" --ai
  ` + brand.BinName() + ` knowledge install team-platform
  ` + brand.BinName() + ` knowledge remove --context team-platform
  ` + brand.BinName() + ` knowledge list`,
	}

	cmd.AddCommand(
		newKnowledgeIndexCmd(),
		newKnowledgeWatchCmd(),
		newKnowledgeQueryCmd(),
		newKnowledgeLintCmd(),
		newKnowledgeSchemaCmd(),
		newKnowledgeInstallCmd(),
		newKnowledgeRemoveCmd(),
		newKnowledgeSyncCmd(),
		newKnowledgeExportCmd(),
		newKnowledgeListCmd(),
		newModuleRuleCmd("knowledge"),
	)

	return cmd
}

func newKnowledgeIndexCmd() *cobra.Command {
	var (
		reset      bool
		useLouvain bool
		workers    int
		context    string
	)
	cmd := &cobra.Command{
		Use:   "index [path]",
		Short: "Index docs/ into the knowledge graph and regenerate the wiki",
		Long: `Scan docs/ and build a persistent knowledge graph wiki.

Project flags (--reset, --louvain) apply to the local project index.
Use --context <name> to re-index a specific imported context.

Examples:
  ` + brand.BinName() + ` knowledge index
  ` + brand.BinName() + ` knowledge index --reset --louvain
  ` + brand.BinName() + ` knowledge index --context team-platform`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			path := config.ResolveDocsDir(parseInlineConfig(cmd), loadProjectConfig())
			if len(args) > 0 {
				path = args[0]
			}
			return runKnowledgeIndex(path, workers, reset, useLouvain)
		},
	}
	cmd.Flags().BoolVar(&reset, "reset", false, "Clear graph and re-index from scratch (project only)")
	cmd.Flags().BoolVar(&useLouvain, "louvain", false, "Use Louvain community detection (project only)")
	cmd.Flags().IntVar(&workers, "workers", 0, "Parallel workers (0 = sequential)")
	cmd.Flags().StringVar(&context, "context", "", "Re-index a specific imported context by name")
	return cmd
}

func newKnowledgeQueryCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "query <text>",
		Short: "Search the knowledge wiki using AI",
		Long: `Search the knowledge wiki using the AI consultation cycle.

The wiki module presents index.md to the AI, then cycles through page
requests until the AI has enough context to answer the query. No Cypher
or raw graph access is needed — only the generated wiki is used.

With --context: searches an imported context instead of the project wiki.

Examples:
  ` + brand.BinName() + ` knowledge query "how does authentication work?"
  ` + brand.BinName() + ` knowledge query "auth patterns" --context team-platform`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeQuery(args[0], context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Query an imported context by name")
	return cmd
}

func newKnowledgeInstallCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "install <name>",
		Short: "Import an external knowledge context from the hub",
		Long: `Fetch another project's knowledge artifact from the hub (wiki)
and install it locally at ` + brand.DotDir() + `/knowledge/<name>/.

The hub artifact already contains the built wiki and graph — no re-indexing needed.

Examples:
  ` + brand.BinName() + ` knowledge install team-platform`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeImport(args[0], false, false)
		},
	}
	return cmd
}

func newKnowledgeRemoveCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "remove",
		Short: "Remove the project knowledge graph or an imported context",
		Long: `Without --context: clears the project knowledge graph (source files kept).
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
	var context string
	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Re-sync an imported context from the global cache",
		Long: `Re-installs files for an imported context from the global cache.
Use --context to specify which context to sync. Without it, syncs all.

Examples:
  ` + brand.BinName() + ` knowledge sync
  ` + brand.BinName() + ` knowledge sync --context team-platform`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeSync(context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Sync a specific imported context by name")
	return cmd
}

func newKnowledgeExportCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "export",
		Short: "Export the project knowledge wiki and graph to the hub",
		Long: `Export the project's knowledge wiki (plain markdown) and wiki
(Parquet via EXPORT DATABASE) so other projects can import it.

Examples:
  ` + brand.BinName() + ` knowledge export`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeExport()
		},
	}
	return cmd
}

func newKnowledgeWatchCmd() *cobra.Command {
	var useLouvain bool
	cmd := &cobra.Command{
		Use:   "watch [path]",
		Short: "Watch docs/ for changes and re-index + regenerate wiki incrementally",
		Long: `Watch a directory for file changes and incrementally re-index modified files,
then regenerate the knowledge wiki. Delegates to the wiki engine's watch mode.

Only project scope is supported for watch (not imported contexts).

Examples:
  ` + brand.BinName() + ` knowledge watch
  ` + brand.BinName() + ` knowledge watch docs/ --louvain`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			path := config.ResolveDocsDir(parseInlineConfig(cmd), loadProjectConfig())
			if len(args) > 0 {
				path = args[0]
			}
			return runKnowledgeWatch(path, useLouvain)
		},
	}
	cmd.Flags().BoolVar(&useLouvain, "louvain", false, "Use Louvain community detection on wiki regeneration")
	return cmd
}

func newKnowledgeSchemaCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "schema",
		Short: "Show the knowledge graph schema and node properties",
		Long: `Print the knowledge graph schema — node labels, properties, and relationships.
Useful for AI agents to understand the graph structure before writing Cypher queries.

Examples:
  ` + brand.BinName() + ` knowledge schema
  ` + brand.BinName() + ` knowledge schema --context team-platform`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeSchema(context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Show schema for an imported context")
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
		deep      bool
		fix       bool
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

With --fix: auto-repairs broken backlinks by re-injecting cross-references.
With --deep: enables AI-assisted contradiction detection (uses tokens).

Examples:
  ` + brand.BinName() + ` knowledge lint
  ` + brand.BinName() + ` knowledge lint --fix
  ` + brand.BinName() + ` knowledge lint --deep --stale-days 7`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runKnowledgeLint(context, deep, fix, staleDays)
		},
	}
	cmd.Flags().BoolVar(&deep, "deep", false, "Enable AI-assisted contradiction detection")
	cmd.Flags().BoolVar(&fix, "fix", false, "Auto-repair fixable issues (backlinks)")
	cmd.Flags().IntVar(&staleDays, "stale-days", 30, "Mark pages older than N days as stale")
	cmd.Flags().StringVar(&context, "context", "", "Lint an imported context by name")
	return cmd
}
