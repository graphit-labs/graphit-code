package commands

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/improvements"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newImprovementsCmd() *cobra.Command {
	binName := brand.BinName()

	cmd := &cobra.Command{
		Use:   "improvements",
		Short: "Code improvement analysis methodology.",
		Long: brand.DisplayName + ` Improvements — engineering analysis methodology.

The Improvements module provides the code analysis rules used by both the
Dream module (autonomous) and the IDE agent (on-demand). It covers:

  • Clean Code (DRY, YAGNI, KISS, naming, SRP, error handling)
  • Security (data exposure, injection, auth, crypto)
  • Concurrency & Idempotency (races, resource leaks, replay safety)
  • Cloud Readiness — Twelve-Factor App (config, stateless, disposability)
  • Observability — MELT + OTel (metrics, events, logs, traces, correlation)
  • General Quality (architecture, docs, tests, performance)
  • Decision Validation Gate (respect prior decisions before changing code)

Commands:
  rules     Output the resolved improvement analysis rules
  rule      Manage or display the IDE rule for the improvements module

Examples:
  ` + binName + ` improvements rules
  ` + binName + ` improvements rules --default
  ` + binName + ` improvements rule
  ` + binName + ` improvements rule --default
  ` + binName + ` improvements rule --unset`,
	}

	cmd.AddCommand(
		newImprovementsRulesCmd(),
		newModuleRuleCmd("improvements"),
	)

	return cmd
}

func newImprovementsRulesCmd() *cobra.Command {
	var (
		showDefault bool
		unset       bool
	)

	binName := brand.BinName()

	cmd := &cobra.Command{
		Use:   "rules [file]",
		Short: "Output or manage the improvement analysis methodology rules",
		Long: `Output the resolved code improvement analysis rules.

Resolution order (first match wins):
  1. .<brand>/rules/improvements.md   (project-level)
  2. ~/.<brand>/rules/improvements.md (user-level global)
  3. Compiled-in default

Use --default to output the compiled-in default rules, ignoring any
customization.

Provide a file argument to set a custom rule override. The file will be
saved to ~/` + brand.DotDir() + `/rules/improvements.md.

Use --unset to remove the custom rule and restore the default.

Examples:
  ` + binName + ` improvements rules              # resolved rules
  ` + binName + ` improvements rules --default     # compiled-in default
  ` + binName + ` improvements rules my-rules.md   # set custom rules
  ` + binName + ` improvements rules --unset        # restore default`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			if showDefault {
				p.Data(improvements.DefaultRules())
				return nil
			}

			if len(args) == 0 && !unset {
				p.Data(improvements.Rules())
				return nil
			}

			ide := resolveIDEFlag(cmd)
			filePath := ""
			if len(args) > 0 {
				filePath = args[0]
			}
			return runModuleRuleSet("improvements", filePath, ide, unset)
		},
	}

	cmd.Flags().BoolVar(&showDefault, "default", false, "Show compiled-in default rules (ignore customization)")
	cmd.Flags().BoolVar(&unset, "unset", false, "Remove the custom rule and restore the default")
	return cmd
}
