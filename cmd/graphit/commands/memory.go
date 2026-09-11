package commands

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/spf13/cobra"
)

func newMemoryCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "memory",
		Aliases: []string{"mem"},
		Short:   "Persistent agent memory backed by one authoritative LanceDB table per scope.",
		Long: brand.DisplayName + ` Memory — persistent agent memories in authoritative LanceDB tables.

Memories are stored in the shared memory bucket under one prefix per scope,
with full-text and vector indexes in the same table as their revision history.

Scopes:
  project  Tied to the current project (default).
  user     Belongs to the user, cross-project (--user flag).

Commands:
  index    Refresh indexes on the authoritative memory table
  query    Query memories using AI natural language
  install  Validate and prepare an external authoritative memory context
  remove   Forget local context state without deleting authoritative memory
  sync     Refresh indexes on an imported authoritative context
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
		Short: "Refresh indexes on the authoritative memory table",
		Long: `Ensure full-text and scalar indexes directly on the authoritative memory table.

--reset is retained for compatibility and refreshes the same table indexes in place.

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
	cmd.Flags().BoolVar(&reset, "reset", false, "Refresh authoritative indexes in place")
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
		Short: "Search the authoritative memory table using AI",
		Long: `Search the authoritative memory table and synthesize an answer from matching records.

The records, revision history, full-text indexes and vectors live in the same LanceDB table.

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
		Short: "Prepare memory from an external project",
		Long: `Open another project's authoritative memory table and ensure its direct search indexes.

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
		Short: "Refresh an imported memory context",
		Long: `Ensure direct search indexes on an imported context's authoritative table.

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
	cmd.Flags().BoolVar(&important, "important", false, "Mark as important (surfaced in Agent rule)")
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
		Short: "Search the authoritative memory table by keyword (BM25, no AI)",
		Long: `Search the authoritative memory table with BM25 ranking. No AI is involved.

Returns matching memory IDs or id/revision-id keys with score and title. Writes refresh
the same table's indexes; '` + brand.BinName() + ` memory index' is only needed for repair.

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
full content. These are the memories surfaced in the Agent global rule.

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
		Short: "Mark a memory as important (surfaces in Agent rule)",
		Long: `Set important: true in a memory's frontmatter, making it visible
in the Agent global rule's "Key Project Memories" section.

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
from the Agent global rule's "Key Project Memories" section.

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
