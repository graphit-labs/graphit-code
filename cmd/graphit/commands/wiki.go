package commands

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/spf13/cobra"
)

func newWikiCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "wiki",
		Aliases: []string{"w"},
		Short:   "Multi-wiki AI search — search, chat, and manage sessions across wikis.",
		Long: brand.DisplayName + ` Wiki — AI-powered search across multiple knowledge wikis.

Search project knowledge, memory, ecosystem projects, and hub artifacts
simultaneously using BM25 pre-filtering and AI consultation cycles.

Commands:
  search     Search across one or more wikis with AI
  chat       Interactive chat over wiki context (continues a session)
  sessions   List or delete wiki search sessions
  browse     Browse wiki documents in a structured format
  log        Show wiki sync history
  xrefs      Show cross-references for an entity

Examples:
  ` + brand.BinName() + ` wiki search "how does auth work?"
  ` + brand.BinName() + ` wiki search "auth flow" --wiki project,memory
  ` + brand.BinName() + ` wiki search "deployment" --hub team-platform@latest
  ` + brand.BinName() + ` wiki chat --continue
  ` + brand.BinName() + ` wiki sessions
  ` + brand.BinName() + ` wiki browse --wiki project
  ` + brand.BinName() + ` wiki log --limit 5
  ` + brand.BinName() + ` wiki xrefs "auth-flow"`,
	}

	cmd.AddCommand(
		newWikiSearchCmd(),
		newWikiChatCmd(),
		newWikiSessionsCmd(),
		newWikiBrowseCmd(),
		newWikiLogCmd(),
		newWikiXRefsCmd(),
		newWikiEmbedCmd(),
	)

	return cmd
}

func newWikiSearchCmd() *cobra.Command {
	var (
		wikiRefs        []string
		hubRefs         []string
		sessionName     string
		continueSession bool
		topK            int
		searchMode      string
		aiOptimized     bool
	)
	cmd := &cobra.Command{
		Use:   "search <query>",
		Short: "Search across one or more wikis with AI",
		Long: `Search multiple wiki sources simultaneously using BM25 pre-filtering and
AI consultation cycles. Each wiki source is searched independently and
results are merged for the AI to synthesize a unified answer.

Wiki sources (--wiki):
  project    — the project's knowledge wiki (docs/)
  memory     — the project's memory wiki
  <id>       — another ecosystem project (looked up in global lock)

Hub sources (--hub):
  <artifact-id>[@<version>]  — auto-downloaded from the hub registry

Sessions:
  Each search creates a persistent session that can be continued with
  ` + brand.BinName() + ` wiki chat --continue or by session ID.

Examples:
  ` + brand.BinName() + ` wiki search "how does authentication work?"
  ` + brand.BinName() + ` wiki search "auth flow" --wiki project,memory
  ` + brand.BinName() + ` wiki search "deployment" --wiki project --hub team-platform
  ` + brand.BinName() + ` wiki search "patterns" --top-k 10 --continue`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiSearch(args[0], wikiRefs, hubRefs, sessionName, continueSession, topK, searchMode, aiOptimized)
		},
	}
	cmd.Flags().StringSliceVar(&wikiRefs, "wiki", []string{"project"}, "Wiki sources: project, memory, or ecosystem project ID (comma-separated)")
	cmd.Flags().StringSliceVar(&hubRefs, "hub", nil, "Hub knowledge artifacts: artifact-id[@version] (comma-separated, auto-downloaded)")
	cmd.Flags().StringVar(&sessionName, "session", "", "Name for the search session")
	cmd.Flags().BoolVar(&continueSession, "continue", false, "Continue the most recent session")
	cmd.Flags().IntVar(&topK, "top-k", 0, "BM25 results per wiki source (0 = no limit)")
	cmd.Flags().StringVar(&searchMode, "mode", "hybrid", "Search mode: hybrid (default, FTS + semantic), fts (keyword only), semantic (vector only)")
	cmd.Flags().BoolVar(&aiOptimized, "ai-optimized", false, "Output in compact, token-efficient format (use --ai-optimized for TOON format)")
	return cmd
}

func newWikiChatCmd() *cobra.Command {
	var (
		sessionID       string
		continueSession bool
	)
	cmd := &cobra.Command{
		Use:   "chat",
		Short: "Interactive chat over wiki context",
		Long: `Start an interactive chat session over wiki context. Continues an
existing session to ask follow-up questions about previous search results.

The chat engine maintains conversation history and wiki source context
across turns, allowing natural multi-turn exploration of documentation.

Commands:
  /exit    — end the chat session
  Ctrl+D   — end the chat session

Examples:
  ` + brand.BinName() + ` wiki chat --continue
  ` + brand.BinName() + ` wiki chat --session 01HXYZ...`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiChat(sessionID, continueSession)
		},
	}
	cmd.Flags().StringVar(&sessionID, "session", "", "Continue a specific session by ID")
	cmd.Flags().BoolVar(&continueSession, "continue", false, "Continue the most recent session")
	return cmd
}

func newWikiSessionsCmd() *cobra.Command {
	var deleteID string
	cmd := &cobra.Command{
		Use:   "sessions",
		Short: "List or delete wiki search sessions",
		Long: `List all wiki search sessions for this project, or delete a specific session.

Sessions are stored globally and associated with the current project directory.
Each session contains the conversation history, wiki sources, and search context.

Examples:
  ` + brand.BinName() + ` wiki sessions
  ` + brand.BinName() + ` wiki sessions --delete 01HXYZ...`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiSessions(deleteID)
		},
	}
	cmd.Flags().StringVar(&deleteID, "delete", "", "Delete a specific session by ID")
	return cmd
}

func newWikiBrowseCmd() *cobra.Command {
	var (
		wikiScope   string
		docType     string
		limit       int
		aiOptimized bool
	)
	cmd := &cobra.Command{
		Use:   "browse",
		Short: "Browse wiki documents in a structured format",
		Long: `Browse wiki chunks/documents stored in the WikiDB. Lists entries in a
structured format, replacing the need to read index.md directly.

Examples:
  ` + brand.BinName() + ` wiki browse
  ` + brand.BinName() + ` wiki browse --wiki memory
  ` + brand.BinName() + ` wiki browse --type specification --limit 20`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiBrowse(wikiScope, docType, limit, aiOptimized)
		},
	}
	cmd.Flags().StringVar(&wikiScope, "wiki", "project", "Wiki scope: project or memory")
	cmd.Flags().StringVar(&docType, "type", "", "Filter by document type (e.g., specification, architecture)")
	cmd.Flags().IntVar(&limit, "limit", 100, "Max results to return")
	cmd.Flags().BoolVar(&aiOptimized, "ai-optimized", false, "Output in compact, token-efficient format (use --ai-optimized for TOON format)")
	return cmd
}

func newWikiLogCmd() *cobra.Command {
	var (
		wikiScope   string
		limit       int
		aiOptimized bool
	)
	cmd := &cobra.Command{
		Use:   "log",
		Short: "Show wiki sync history",
		Long: `Show the sync history for a wiki database. Displays a timeline of sync
operations, including what was added, updated, and deleted.

Examples:
  ` + brand.BinName() + ` wiki log
  ` + brand.BinName() + ` wiki log --wiki memory
  ` + brand.BinName() + ` wiki log --limit 5`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiLog(wikiScope, limit, aiOptimized)
		},
	}
	cmd.Flags().StringVar(&wikiScope, "wiki", "project", "Wiki scope: project or memory")
	cmd.Flags().IntVar(&limit, "limit", 10, "Max log entries to show")
	cmd.Flags().BoolVar(&aiOptimized, "ai-optimized", false, "Output in compact, token-efficient format (use --ai-optimized for TOON format)")
	return cmd
}

func newWikiXRefsCmd() *cobra.Command {
	var (
		wikiScope   string
		depth       int
		aiOptimized bool
	)
	cmd := &cobra.Command{
		Use:   "xrefs <query>",
		Short: "Show cross-references for an entity",
		Long: `Show inbound and outbound cross-references for a wiki entity slug.
Uses the WikiDB xrefs table to find related documents.

Examples:
  ` + brand.BinName() + ` wiki xrefs auth-flow
  ` + brand.BinName() + ` wiki xrefs "database-schema" --depth 2
  ` + brand.BinName() + ` wiki xrefs config --wiki memory`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiXRefs(args[0], wikiScope, depth, aiOptimized)
		},
	}
	cmd.Flags().StringVar(&wikiScope, "wiki", "project", "Wiki scope: project or memory")
	cmd.Flags().IntVar(&depth, "depth", 1, "Depth of graph traversal (1-3)")
	cmd.Flags().BoolVar(&aiOptimized, "ai-optimized", false, "Output in compact, token-efficient format (use --ai-optimized for TOON format)")
	return cmd
}

func newWikiEmbedCmd() *cobra.Command {
	var embedWikiScope string

	cmd := &cobra.Command{
		Use:   "embed",
		Short: "Generate vector embeddings for wiki semantic search",
		Long: `Generate or update vector embeddings for wiki document chunks.
Embeddings enable semantic search (` + brand.BinName() + ` wiki search --mode semantic).`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runWikiEmbed(embedWikiScope)
		},
	}
	cmd.Flags().StringVar(&embedWikiScope, "wiki", "project", "Wiki scope: project or memory")
	return cmd
}
