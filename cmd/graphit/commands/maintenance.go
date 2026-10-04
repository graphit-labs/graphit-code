package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/storework"
	"github.com/spf13/cobra"
)

// The commands in this file are the CLI form of work the daemon schedules. The daemon
// runs each of them as its own process; an operator can run the same command by hand.

func commandContext(cmd *cobra.Command) context.Context {
	if ctx := cmd.Context(); ctx != nil {
		return ctx
	}
	return context.Background()
}

func workingDir() (string, error) {
	wd, err := os.Getwd()
	if err != nil {
		return "", err
	}
	return filepath.Abs(wd)
}

func newMemoryMaintainCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "maintain",
		Short: "Compact the authoritative memory table and prune old versions",
		Long: `Fold recent writes into the indexes of the authoritative memory table, compact
it and prune physical versions under the retention policy.

The daemon runs this periodically for every active project and for the user scope.

Examples:
  ` + brand.BinName() + ` memory maintain
  ` + brand.BinName() + ` memory maintain --user`,
		RunE: func(cmd *cobra.Command, args []string) error {
			wd, err := workingDir()
			if err != nil {
				return err
			}
			return storework.MaintainMemory(commandContext(cmd), wd, userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Maintain the user-scope memory table")
	return cmd
}

func newMemoryEmbedCmd() *cobra.Command {
	var userScope bool
	cmd := &cobra.Command{
		Use:   "embed",
		Short: "Generate vector embeddings for memories that lack them",
		Long: `Fill the vector column of the authoritative memory table for memories that are
new or changed since the last run.

The daemon runs this periodically for every active project and for the user scope.

Examples:
  ` + brand.BinName() + ` memory embed
  ` + brand.BinName() + ` memory embed --user`,
		RunE: func(cmd *cobra.Command, args []string) error {
			wd, err := workingDir()
			if err != nil {
				return err
			}
			return storework.EmbedMemory(commandContext(cmd), wd, userScope)
		},
	}
	cmd.Flags().BoolVar(&userScope, "user", false, "Embed the user-scope memory table")
	return cmd
}

func newTaskMaintainCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "maintain",
		Short: "Compact the task tables and refresh their indexes",
		Long: `Compact the authoritative task and session tables of the current project and
refresh their indexes. The daemon runs this periodically for every active project.`,
		Args:    cobra.NoArgs,
		PreRunE: requireProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			wd, err := workingDir()
			if err != nil {
				return err
			}
			return storework.MaintainTasks(commandContext(cmd), wd)
		},
	}
}

func newDreamRunCmd() *cobra.Command {
	var sessionID, agent string
	cmd := &cobra.Command{
		Use:   "run",
		Short: "Run one Dream session now",
		Long: `Run one Dream Memory-consolidation session for the current project now.

The daemon decides when a project is idle enough to dream and runs the session
through this command; its outcome lands in the Dream run ledger shown by
'` + brand.BinName() + ` dream status'. Without --session a new session id is used.`,
		Args:    cobra.NoArgs,
		PreRunE: requireSetupAndProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			wd, err := os.Getwd()
			if err != nil {
				return err
			}
			if agent == "" {
				projectCfg, lockfileAgents := loadProjectLockInfo()
				agent = config.ResolveProjectAgent("", parseInlineConfig(cmd), projectCfg, lockfileAgents)
			}
			if sessionID == "" {
				sessionID = dream.NewSessionID()
			}
			p := output.NewPrinter("dream")
			return dream.ExecuteSession(commandContext(cmd), wd, agent, sessionID, func(format string, args ...any) {
				p.Step(format, args...)
			})
		},
	}
	cmd.Flags().StringVar(&sessionID, "session", "", "Dream session id to run or resume")
	cmd.Flags().StringVar(&agent, "agent", "", "Agent CLI that runs the session (default: the project agent)")
	return cmd
}

// hydrationReport is the machine-readable result of `hub hydrate --json`.
type hydrationReport struct {
	ASTBaseChanged       bool `json:"ast_base_changed"`
	KnowledgeBaseChanged bool `json:"knowledge_base_changed"`
}

func newHubHydrateCmd() *cobra.Command {
	var asJSON bool
	cmd := &cobra.Command{
		Use:   "hydrate",
		Short: "Refresh the project's published Lance base from the Hub",
		Long: `Check the Hub for a newer published AST and Knowledge base of the current
project's branch and, when one exists, replace the local generated store with it.
Local changes are reconciled into that base by the next index.

--json reports which bases changed; the daemon uses it to decide whether a full
reindex is needed.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			wd, err := os.Getwd()
			if err != nil {
				return err
			}
			wd, _ = filepath.Abs(wd)
			result, err := hub.HydrateProjectLanceWithResult(commandContext(cmd), wd, loadProjectConfig())
			if err != nil {
				return fmt.Errorf("hydrate published Lance base: %w", err)
			}
			report := hydrationReport{ASTBaseChanged: result.ASTBaseChanged, KnowledgeBaseChanged: result.KnowledgeBaseChanged}
			if asJSON {
				return json.NewEncoder(cmd.OutOrStdout()).Encode(report)
			}
			p := output.NewPrinter("")
			switch {
			case report.ASTBaseChanged || report.KnowledgeBaseChanged:
				p.Success("Published base refreshed (AST: %t, Knowledge: %t)", report.ASTBaseChanged, report.KnowledgeBaseChanged)
			default:
				p.Success("Published base is current")
			}
			return nil
		},
	}
	cmd.Flags().BoolVar(&asJSON, "json", false, "Print which bases changed as JSON")
	return cmd
}
