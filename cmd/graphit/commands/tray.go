package commands

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/tray"
	"github.com/spf13/cobra"
)

func newTrayCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "tray",
		Short: "Show daemon status and controls in the system tray",
		RunE:  func(cmd *cobra.Command, args []string) error { return tray.Run() },
	}
	login := &cobra.Command{Use: "login", Short: "Manage tray startup at user login"}
	login.AddCommand(
		&cobra.Command{Use: "enable", Short: "Show the tray after user login", RunE: func(cmd *cobra.Command, args []string) error {
			return tray.InstallLogin()
		}},
		&cobra.Command{Use: "disable", Short: "Disable tray startup at login", RunE: func(cmd *cobra.Command, args []string) error {
			return tray.RemoveLogin()
		}},
		&cobra.Command{Use: "status", Short: "Show tray login setting", RunE: func(cmd *cobra.Command, args []string) error {
			enabled, err := tray.LoginEnabled()
			if err != nil {
				return err
			}
			_, err = fmt.Fprintf(cmd.OutOrStdout(), "Tray at login: %t\n", enabled)
			return err
		}},
	)
	cmd.AddCommand(login)
	return cmd
}
