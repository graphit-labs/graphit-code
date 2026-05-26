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

Examples:
  ` + brand.BinName() + ` wiki search "how does auth work?"
  ` + brand.BinName() + ` wiki search "auth flow" --wiki project,memory
  ` + brand.BinName() + ` wiki search "deployment" --hub team-platform@latest
  ` + brand.BinName() + ` wiki chat --continue
  ` + brand.BinName() + ` wiki sessions`,
	}

	cmd.AddCommand(
		newWikiSearchCmd(),
		newWikiChatCmd(),
		newWikiSessionsCmd(),
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
			return runWikiSearch(args[0], wikiRefs, hubRefs, sessionName, continueSession, topK)
		},
	}
	cmd.Flags().StringSliceVar(&wikiRefs, "wiki", []string{"project"}, "Wiki sources: project, memory, or ecosystem project ID (comma-separated)")
	cmd.Flags().StringSliceVar(&hubRefs, "hub", nil, "Hub knowledge artifacts: artifact-id[@version] (comma-separated, auto-downloaded)")
	cmd.Flags().StringVar(&sessionName, "session", "", "Name for the search session")
	cmd.Flags().BoolVar(&continueSession, "continue", false, "Continue the most recent session")
	cmd.Flags().IntVar(&topK, "top-k", 0, "BM25 results per wiki source (0 = no limit)")
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
