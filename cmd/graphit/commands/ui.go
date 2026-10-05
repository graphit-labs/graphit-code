package commands

import (
	"context"
	"fmt"
	"time"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newUICmd() *cobra.Command {
	var repoPath string
	cmd := &cobra.Command{
		Use:   "ui",
		Short: "Open the global Graphit UI, starting its daemon server on demand",
		Long: `Open the global Graphit web UI served by the daemon.

The daemon starts its UI only when requested, unless GRAPHIT_MODULES_DAEMON_UI=true.
Repeated requests reuse that server. Projects are selected in the interface.
Use --repo <directory> to run an explicit standalone project UI in the foreground.`,
		Aliases: []string{"serve"},
		RunE: func(cmd *cobra.Command, args []string) error {
			if repoPath != "" {
				return runUnifiedServe(repoPath)
			}
			return runRequestedUI(cmd.Context(), daemonctl.IsRunning, daemonctl.EnsureRunning, daemonctl.RequestUIURL, openBrowser)
		},
	}
	cmd.Flags().StringVar(&repoPath, "repo", "", "Run a standalone UI for this explicit project directory")
	return cmd
}

func runRequestedUI(ctx context.Context, running func() bool, ensure func() (bool, error), request func(context.Context) (string, error), browser func(string)) error {
	if !running() {
		if _, err := ensure(); err != nil {
			return fmt.Errorf("starting daemon for UI: %w", err)
		}
	}
	ctx, cancel := context.WithTimeout(ctx, 35*time.Second)
	defer cancel()
	address, err := request(ctx)
	if err != nil {
		return err
	}
	output.NewPrinter("").Success("Graphit UI → %s", address)
	if !nonInteractive {
		browser(address)
	}
	return nil
}
