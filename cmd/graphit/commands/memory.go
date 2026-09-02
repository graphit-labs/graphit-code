package commands

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/spf13/cobra"
)

func newMemoryCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "memory",
		Aliases: []string{"mem"},
		Short:   "Memory wiki — insert, delete, query, and manage persistent agent memories.",
		Long: brand.DisplayName + ` Memory — LLM wiki for persistent agent memories.

Memories are stored in the shared memory bucket under one prefix per scope,
and exposed as a navigable LLM wiki.

Scopes:
  project  Tied to the current project (default).
  user     Belongs to the user, cross-project (--user flag).

Commands:
  index    Synchronize the compiled wiki from the memory table
  query    Query the memory graph (Cypher or AI natural language)
  install  Import an external memory context from another project
  remove   Remove the project memory graph or an imported context
  sync     Recompile an imported context from its table
  list     List memories
  rule     Customize the global memory agent rule
  insert   Add a new memory entry
  delete   Delete a memory entry by slug

Examples:
  ` + brand.BinName() + ` memory insert "API keys must go in .env"
  ` + brand.BinName() + ` memory insert "prefer functional style" --user
  ` + brand.BinName() + ` memory delete my-slug
  ` + brand.BinName() + ` memory list
  ` + brand.BinName() + ` memory query "auth conventions" --ai
  ` + brand.BinName() + ` memory index`,
	}

	cmd.AddCommand(
		newMemoryIndexCmd(),
		newMemoryQueryCmd(),
		newMemorySchemaCmd(),
		newMemoryInstallCmd(),
		newMemoryRemoveCmd(),
		newMemorySyncCmd(),
		newMemoryListCmd(),
		newMemoryInsertCmd(),
		newMemoryUpdateCmd(),
		newMemoryDeleteCmd(),
		newMemorySearchCmd(),
		newMemoryImportantCmd(),
		newMemoryMandatoryCmd(),
		newMemoryPromoteCmd(),
		newMemoryDemoteCmd(),
		newMemoryMarkMandatoryCmd(),
		newMemoryUnmarkMandatoryCmd(),
		newMemoryConsolidateCmd(),
		newModuleRuleCmd("memory"),
	)

	return cmd
}

func newMemoryIndexCmd() *cobra.Command {
	var userScope bool
	var reset bool
	cmd := &cobra.Command{
		Use:   "index",
		Short: "Synchronize the compiled wiki from the memory table",
		Long: `Compile the authoritative memory table into its local searchable wiki.

--reset clears the derived wiki first and compiles it again from the table.

Discarding the wiki is safe because none of it is source. The memories live in
their own store, outside the wiki; the wiki is derived from them.

To re-index an imported context, use '` + brand.BinName() + ` memory sync --context <name>'.

Examples:
  ` + brand.BinName() + ` memory index
  ` + brand.BinName() + ` memory index --user
  ` + brand.BinName() + ` memory index --reset`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryIndex(userScope, reset)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Index user-scope memories (cross-project)")
	cmd.Flags().BoolVar(&reset, "reset", false, "Clear the wiki and rebuild it from the memories")
	return cmd
}

func newMemorySchemaCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "schema",
		Short: "Show the memory graph schema and node properties",
		Long: `Print the memory graph schema — node labels, properties, and relationships.
Useful for AI agents to understand the graph structure before writing Cypher queries.

Examples:
  ` + brand.BinName() + ` memory schema
  ` + brand.BinName() + ` memory schema --context team-shared-lib`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemorySchema(context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Show schema for an imported memory context")
	return cmd
}

func newMemoryQueryCmd() *cobra.Command {
	var (
		userScope bool
		context   string
	)
	cmd := &cobra.Command{
		Use:   "query <question>",
		Short: "Search the memory wiki using AI",
		Long: `Search the memory wiki using the AI consultation cycle.

The wiki module renders the catalogue from LanceDB for the AI, then cycles through page
requests until the AI has enough context to answer the query. No Cypher
or raw graph access is needed — only the generated wiki is used.

With --user: searches user-scope memories instead of project scope.
With --context: searches an imported external memory context.

Examples:
  ` + brand.BinName() + ` memory query "auth conventions we follow"
  ` + brand.BinName() + ` memory query "postgres usage" --user
  ` + brand.BinName() + ` memory query "auth patterns" --context team-api`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryQuery(args[0], userScope, context)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Search user-scope memories")
	cmd.Flags().StringVar(&context, "context", "", "Search an imported external memory context by name")
	return cmd
}

func newMemoryInstallCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "install <project-id-or-name>",
		Short: "Import and sync memory from an external project",
		Long: `Open another project's memory table and compile its local query projection.

Examples:
  ` + brand.BinName() + ` memory install team-shared-lib
  ` + brand.BinName() + ` memory install abc123-project-id`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryImport(args[0])
		},
	}
	return cmd
}

func newMemoryRemoveCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "remove",
		Short: "Remove the project memory graph or an imported context",
		Long: `Without --context: clears the project memory graph (source files kept).
With --context <name>: removes the named imported memory context from this project.

Examples:
  ` + brand.BinName() + ` memory remove
  ` + brand.BinName() + ` memory remove --context team-shared-lib`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if context != "" {
				return runMemoryRemoveContext(context)
			}
			return runMemoryClean()
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Name of the imported memory context to remove")
	return cmd
}

func newMemorySyncCmd() *cobra.Command {
	var context string
	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Recompile an imported memory context",
		Long: `Synchronize an imported context's local wiki from its authoritative table.

Examples:
  ` + brand.BinName() + ` memory sync --context team-shared-lib`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemorySync(context)
		},
	}
	cmd.Flags().StringVar(&context, "context", "", "Sync a specific imported context by name")
	return cmd
}

func newMemoryListCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List memories in the project or user scope",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryList(userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "List user-scope memories")
	return cmd
}

func newMemoryInsertCmd() *cobra.Command {
	var (
		content     string
		userScope   bool
		linkProject bool
		important   bool
		mandatory   bool
		memType     string
		tags        string
	)
	cmd := &cobra.Command{
		Use:   "insert <title>",
		Short: "Add a new memory entry",
		Long: `Create a new persistent memory record in the memory store.

Without --user: scoped to the current project (default).
With --user: global user memory (cross-project).
With --user --project: user memory explicitly linked to the current project.
With --important: mark as important.
With --mandatory: load unconditionally at every session start.
With --type: classify the memory (convention, correction, decision, tension, fact, skill).
With --tags: add cross-cutting tags (comma-separated).

Examples:
  ` + brand.BinName() + ` memory insert "API keys must go in .env" --type convention --important
  ` + brand.BinName() + ` memory insert "prefer functional style" --user --type convention
  ` + brand.BinName() + ` memory insert "always use ULID for IDs" --type convention --important --tags "ids,database"
  ` + brand.BinName() + ` memory insert "chose Postgres over MongoDB" --type tension --content "Chose: Postgres\nOver: MongoDB\nBecause: ACID compliance\nAccepting: Higher operational complexity"
  ` + brand.BinName() + ` memory insert "fix: restart dev server after .env change" --type skill`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryAdd(args[0], content, userScope, linkProject, important, mandatory, memType, tags)
		},
	}
	cmd.Flags().StringVar(&content, "content", "", "Memory body text (optional; title is sufficient)")
	cmd.Flags().BoolVar(&userScope, "user", false, "User scope (cross-project)")
	cmd.Flags().BoolVar(&linkProject, "project", false, "Associate user memory with the current project (requires --user)")
	cmd.Flags().BoolVar(&important, "important", false, "Mark as important (surfaced in IDE rule)")
	cmd.Flags().BoolVar(&mandatory, "mandatory", false, "Mark as mandatory for unconditional session-start recall")
	cmd.Flags().StringVar(&memType, "type", "", "Memory type: convention, correction, decision, tension, fact, skill")
	cmd.Flags().StringVar(&tags, "tags", "", "Comma-separated tags for cross-cutting grouping")
	registerMemoryTypeFlagCompletion(cmd)
	return cmd
}

func newMemoryUpdateCmd() *cobra.Command {
	var (
		content   string
		title     string
		userScope bool
	)
	cmd := &cobra.Command{
		Use:   "update <id>",
		Short: "Update an existing memory entry",
		Long: `Modify the content of an existing memory, preserving its ID and creation date.

Examples:
  ` + brand.BinName() + ` memory update 01JK3ABC --content "Updated convention details"
  ` + brand.BinName() + ` memory update 01JK3ABC --title "New title" --content "New body"
  ` + brand.BinName() + ` memory update 01JK3ABC --content "refreshed" --user`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryUpdate(args[0], content, title, userScope)
		},
	}
	cmd.Flags().StringVar(&content, "content", "", "New memory body text")
	cmd.Flags().StringVar(&title, "title", "", "New title (optional; keeps old title if omitted)")
	cmd.Flags().BoolVar(&userScope, "user", false, "Update in user scope")
	return cmd
}

func newMemorySearchCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "search <term>",
		Short: "Search the memory wiki by keyword (BM25, no AI)",
		Long: `Search the compiled memory wiki with BM25 ranking. No AI is involved.

Returns the matching wiki pages with their score and title — page slugs, not memory
IDs. It reads the compiled wiki rather than the raw memory files, which is what makes
it ranked and cheap. It is also why a memory written moments ago may not appear yet:
it is in the store, but the wiki has not recompiled. Use '` + brand.BinName() + ` memory list' to see
the store itself, or '` + brand.BinName() + ` memory index' to force the rebuild.

Examples:
  ` + brand.BinName() + ` memory search "authentication"
  ` + brand.BinName() + ` memory search "postgres" --user`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemorySearch(args[0], userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Search user-scope memories")
	return cmd
}

func newMemoryDeleteCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "delete <id>",
		Short: "Delete a memory entry by ID",
		Long: `Remove a memory from the store by its ID — the ULID shown by ` + brand.BinName() + ` memory list.

Not a slug: memory files are named by ID, so a slug finds nothing.

Without --user: removes from the project scope.
With --user: removes from the user scope.

Examples:
  ` + brand.BinName() + ` memory delete 01KZYN42E0VHB2MC98PKECAN15
  ` + brand.BinName() + ` memory delete 01KZYN42E0VHB2MC98PKECAN15 --user`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryRemove(args[0], userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Delete from user scope")
	return cmd
}

func newMemoryImportantCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "important",
		Short: "List important memories with their content",
		Long: `List all important memories (frontmatter carrying important: true) with their
full content. These are the memories surfaced in the IDE global rule.

Without --user: lists important project memories (default).
With --user: lists important user memories.

Examples:
  ` + brand.BinName() + ` memory important
  ` + brand.BinName() + ` memory important --user`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryImportantList(userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "List important user-scope memories")
	return cmd
}

func newMemoryMandatoryCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "mandatory",
		Short: "List mandatory memories with their content",
		Long: `List every mandatory memory directly from the authoritative store, without search.
These memories form the unconditional first phase of session-start recall.`,
		RunE: func(cmd *cobra.Command, args []string) error { return runMemoryMandatoryList(userScope) },
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "List mandatory user-scope memories")
	return cmd
}

func newMemoryPromoteCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "promote <id>",
		Short: "Mark a memory as important (surfaces in IDE rule)",
		Long: `Set important: true in a memory's frontmatter, making it visible
in the IDE global rule's "Key Project Memories" section.

Examples:
  ` + brand.BinName() + ` memory promote 01JK3ABC
  ` + brand.BinName() + ` memory promote 01JK3ABC --user`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryPromote(args[0], userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Promote a user-scope memory")
	return cmd
}

func newMemoryDemoteCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "demote <id>",
		Short: "Remove important status from a memory",
		Long: `Clear important: true from a memory's frontmatter, removing it
from the IDE global rule's "Key Project Memories" section.

Examples:
  ` + brand.BinName() + ` memory demote 01JK3ABC
  ` + brand.BinName() + ` memory demote 01JK3ABC --user`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryDemote(args[0], userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Demote a user-scope memory")
	return cmd
}

func newMemoryMarkMandatoryCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "mark-mandatory <id>",
		Short: "Make a memory part of unconditional session-start recall",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryMandatoryChange(args[0], userScope, true)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Change a user-scope memory")
	return cmd
}

func newMemoryUnmarkMandatoryCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "unmark-mandatory <id>",
		Short: "Remove a memory from unconditional session-start recall",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryMandatoryChange(args[0], userScope, false)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Change a user-scope memory")
	return cmd
}

func newMemoryConsolidateCmd() *cobra.Command {
	var (
		userScope bool
		dryRun    bool
	)
	cmd := &cobra.Command{
		Use:   "consolidate",
		Short: "Find and resolve duplicate, contradicting and stale memories",
		Long: `Consolidate the memory store: fold duplicates into one memory, resolve
contradictions in favour of what is true now, and flag entries that have gone a long
time without revision.

The analysis runs on the agent CLI configured in 'ai.cli' — it decides which memories
duplicate or contradict which. Every change is then applied here, in Go, under
invariants the analysis cannot override:

  • content is never dropped — a memory is only removed by an action that carried
    its content into a surviving memory
  • importance is never lost — if any memory in a group was important, the survivor is
  • classification is never lost — the survivor keeps the most specific type
  • an important memory is never deleted outright
  • the last remaining memory in a scope is never deleted
  • everything refused is reported, with the reason

Without an AI CLI, only the deterministic staleness check runs.

This is the same consolidation the dream module performs on idle. Run it here when you
want it now instead of waiting, or when the dream module is off.

By default nothing is applied. Use --dry-run=false to apply.

Examples:
  ` + brand.BinName() + ` memory consolidate
  ` + brand.BinName() + ` memory consolidate --dry-run=false
  ` + brand.BinName() + ` memory consolidate --user --dry-run=false`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMemoryConsolidate(userScope, dryRun)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Consolidate user-scope memories")
	cmd.Flags().BoolVar(&dryRun, "dry-run", true, "Only show the plan, change nothing")
	return cmd
}
