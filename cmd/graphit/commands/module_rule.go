package commands

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newModuleRuleCmd(module string) *cobra.Command {
	var (
		unset       bool
		showDefault bool
	)

	binName := brand.BinName()

	cmd := &cobra.Command{
		Use:   "rule [file]",
		Short: "Manage or display the rule for the " + module + " module",
		Long: `Manage the IDE rule block for the ` + module + ` module.

Without arguments: outputs the resolved rule content (respecting any user
customization in ` + brand.GlobalRulesDir() + `/` + module + `.md).

With --default: outputs the compiled-in default rules, ignoring any
customization.

With a file argument: saves the file as a custom rule override at:

  ~/` + brand.DotDir() + `/rules/` + module + `.md

On every ` + binName + ` init or update this file takes precedence over
the built-in rule. The project IDE rules file is updated immediately.

Use --unset to delete the custom rule and restore the default.

Examples:
  ` + binName + ` ` + module + ` rule                  # resolved rules
  ` + binName + ` ` + module + ` rule --default         # compiled-in default
  ` + binName + ` ` + module + ` rule my-custom-rule.md
  ` + binName + ` ` + module + ` rule --unset`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			if showDefault {
				p.Data(getModuleDefaultRule(module))
				return nil
			}

			if len(args) == 0 && !unset {
				p.Data(getModuleResolvedRule(module))
				return nil
			}

			ide := resolveIDEFlag(cmd)
			filePath := ""
			if len(args) > 0 {
				filePath = args[0]
			}
			return runModuleRuleSet(module, filePath, ide, unset)
		},
	}

	cmd.Flags().BoolVar(&unset, "unset", false, "Remove the custom rule and restore the default")
	cmd.Flags().BoolVar(&showDefault, "default", false, "Show compiled-in default rules (ignore customization)")
	return cmd
}
