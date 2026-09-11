package commands

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
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
  index    Index the project docs/ into the knowledge graph and wiki
  query    Query the knowledge graph (Cypher or AI natural language)
  remove   Remove the project knowledge graph or an imported context
  sync     Rebuild the local project wiki
  list     List all installed knowledge contexts
  rule     Customize the global knowledge agent rule

Examples:
  ` + brand.BinName() + ` knowledge index --louvain
  ` + brand.BinName() + ` knowledge query "how does auth work?" --ai
  ` + brand.BinName() + ` hub install team-platform --type knowledge
  ` + brand.BinName() + ` knowledge remove --context team-platform
  ` + brand.BinName() + ` knowledge list`,
	}

	cmd.AddCommand(
		newKnowledgeIndexCmd(),
		newKnowledgeWatchCmd(),
		newKnowledgeQueryCmd(),
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

func newKnowledgeIndexCmd() *cobra.Command {
	var (
		reset      bool
		useLouvain bool
		workers    int
	)
	cmd := &cobra.Command{
		Use:   "index [path]",
		Short: "Index the docs tree into the knowledge graph and regenerate the wiki",
		Long: `Scan the documentation tree and build a persistent knowledge graph wiki.

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

func newKnowledgeSearchCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "search <query>",
		Short: "Search the knowledge wiki using BM25 keyword ranking",
		Long: `Search the knowledge wiki using FTS5 + BM25 keyword ranking.

Returns ranked results without AI — fast, local, and deterministic.
Use 'query' for AI-powered deep consultation.

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
