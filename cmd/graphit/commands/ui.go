package commands

import (
	"github.com/spf13/cobra"
)

func newUICmd() *cobra.Command {
	var repoPath string

	cmd := &cobra.Command{
		Use:   "ui",
		Short: "Start the Graphit UI (Hub + AST all-in-one) and open it in the browser",
		Long: `Start the unified Graphit web UI.

A single server is started on a free port, serving:
  • Hub: registry, install/uninstall, submit, update
  • AST Explorer: graph visualizer, Cypher query, schema, export

The browser is opened automatically. Multiple instances can run
simultaneously, one per project, each on its own port.`,
		Aliases: []string{"serve"},
		RunE: func(cmd *cobra.Command, args []string) error {
			return runUnifiedServe(repoPath)
		},
	}

	cmd.Flags().StringVar(&repoPath, "repo", "", "Repository path to visualize (default: current directory)")
	return cmd
}
