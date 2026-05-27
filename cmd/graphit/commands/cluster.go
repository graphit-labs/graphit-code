package commands

import (
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newClusterCmd() *cobra.Command {
	var flagGet string
	var flagGetAll bool
	var flagUnset string

	cmd := &cobra.Command{
		Use:   "cluster [key] [value]",
		Short: "Manage project cluster labels for ecosystem grouping",
		Long: `Manage cluster labels that group related projects together.

Projects with shared cluster labels are considered siblings and are dynamically resolved,
enabling cross-project discovery via the 'projects' subcommand or MCP tool.

Examples:
  ` + brand.BinName() + ` cluster team backend       # Set label team=backend
  ` + brand.BinName() + ` cluster domain payments     # Set label domain=payments
  ` + brand.BinName() + ` cluster --get team           # Get value of 'team' label
  ` + brand.BinName() + ` cluster --get                # List all labels
  ` + brand.BinName() + ` cluster --unset team         # Remove 'team' label`,
		PreRunE: requireProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			lp := lockfilePath()
			lf, err := hub.LoadLockfile(lp)
			if err != nil || lf == nil {
				return fmt.Errorf("cannot load project lockfile: %v", err)
			}
			projectID := lf.Project.ID
			if projectID == "" {
				return fmt.Errorf("project has no ID — run '%s init' first", brand.BinName())
			}

			mgr, err := hub.NewGlobalLockManager()
			if err != nil {
				return fmt.Errorf("global lock: %v", err)
			}

			wd, _ := os.Getwd()

			if flagUnset != "" {
				if err := mgr.UnsetCluster(projectID, wd, flagUnset); err != nil {
					return fmt.Errorf("unset cluster label: %v", err)
				}
				p.Success("Removed cluster label: %s", flagUnset)
				return nil
			}

			if flagGetAll || flagGet != "" {
				if flagGet != "" {
					vals, err := mgr.GetCluster(projectID, wd, flagGet)
					if err != nil {
						return fmt.Errorf("get cluster label: %v", err)
					}
					if len(vals) == 0 {
						p.StepWarn("Label %q is not set", flagGet)
					} else {
						p.Data(fmt.Sprintf("%s=%s", flagGet, strings.Join(vals, ",")))
					}
				} else {
					labels, err := mgr.GetAllClusterLabels(projectID, wd)
					if err != nil {
						return fmt.Errorf("get cluster labels: %v", err)
					}
					if len(labels) == 0 {
						p.StepWarn("No cluster labels set")
					} else {
						keys := make([]string, 0, len(labels))
						for k := range labels {
							keys = append(keys, k)
						}
						sort.Strings(keys)
						for _, k := range keys {
							p.Data(fmt.Sprintf("%s=%s", k, strings.Join(labels[k], ",")))
						}
					}
				}
				return nil
			}

			if len(args) < 2 {
				return fmt.Errorf("usage: %s cluster <key> <value>\n\nUse --get to read labels or --unset to remove them", brand.BinName())
			}

			key := strings.TrimSpace(args[0])
			value := strings.TrimSpace(args[1])
			if key == "" {
				return fmt.Errorf("cluster key must not be empty")
			}

			if err := mgr.SetCluster(projectID, wd, key, value); err != nil {
				return fmt.Errorf("set cluster label: %v", err)
			}
			p.Success("Set cluster label: %s=%s", key, value)
			return nil
		},
	}

	cmd.Flags().StringVar(&flagGet, "get", "", "Get value of a cluster label (use without value to list all)")
	cmd.Flags().BoolVar(&flagGetAll, "list", false, "List all cluster labels")
	cmd.Flags().StringVar(&flagUnset, "unset", "", "Remove a cluster label")

	cmd.AddCommand(newClusterProjectsCmd())

	return cmd
}

func newClusterProjectsCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "projects",
		Short:   "List sibling projects in the same cluster",
		PreRunE: requireProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			wd, _ := os.Getwd()
			siblings, err := hub.GetClusterProjects(wd)
			if err != nil {
				return err
			}
			if len(siblings) == 0 {
				p.StepWarn("No sibling projects found in the cluster")
				return nil
			}
			p.Header("Cluster Projects")
			keys := make([]string, 0, len(siblings))
			for k := range siblings {
				keys = append(keys, k)
			}
			sort.Strings(keys)
			for _, name := range keys {
				proj := siblings[name]
				p.KeyValue(name, proj.Dir)
				if proj.Description != "" {
					p.Step("Description: %s", proj.Description)
				}
				if len(proj.Cluster) > 0 {
					var labels []string
					for k, v := range proj.Cluster {
						labels = append(labels, fmt.Sprintf("%s=%s", k, strings.Join(v, ",")))
					}
					sort.Strings(labels)
					p.Step("Labels:      %s", strings.Join(labels, " | "))
				}
			}
			return nil
		},
	}
	return cmd
}

