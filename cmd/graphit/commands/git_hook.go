package commands

import (
	"fmt"
	"os"

	"github.com/graphit-labs/graphit-code/internal/githookrun"
	"github.com/spf13/cobra"
)

// newGitHookCmd is the stable entry point used by managed Git hook scripts.
// The script checks for the executable before invoking this command.
func newGitHookCmd() *cobra.Command {
	return &cobra.Command{
		Use:                "_git-hook <hook-name>",
		Hidden:             true,
		DisableFlagParsing: true,
		Args:               cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			projectDir, err := os.Getwd()
			if err != nil {
				return fmt.Errorf("git hook: determine project directory: %w", err)
			}
			return githookrun.Run(cmd.Context(), projectDir, args[0], cmd.InOrStdin(), cmd.OutOrStdout(), cmd.ErrOrStderr(), args[1:]...)
		},
	}
}
