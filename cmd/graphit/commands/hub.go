package commands

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	ideAdapter "github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func openBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	_ = cmd.Start()
}

func newHubCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:               "hub",
		Short:             "Manage knowledge hub artifacts (rules, agents, skills, MCP servers, etc.)",
		PersistentPreRunE: requireSetupAndProject,
	}

	cmd.PersistentFlags().String("ide", "", "Target IDE (antigravity, cursor, claude, kiro, gemini)")
	_ = cmd.RegisterFlagCompletionFunc("ide", completionIDEs())

	cmd.AddCommand(
		newHubInstallCmd(),
		newHubUninstallCmd(),
		newHubLinkCmd(),
		newHubUnlinkCmd(),
		newHubUpdateCmd(),
		newHubListCmd(),
		newHubSearchCmd(),
		newHubShowCmd(),
		newHubSubmitCmd(),
		newHubProjectsCmd(),
		newHubTypePathCmd(),
		newModuleRuleCmd("hub"),
	)

	return cmd
}

func newHubInstallCmd() *cobra.Command {
	var alias string
	var artType string

	cmd := &cobra.Command{
		Use:   "install <artifact-id[@version]>",
		Short: "Install an artifact from the hub",
		Args:  cobra.MinimumNArgs(1),
		Example: `  ` + brand.BinName() + ` hub install my-rule
  ` + brand.BinName() + ` hub install my-agent@1.2.0
  ` + brand.BinName() + ` hub install my-skill --alias my-custom-name`,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			for _, id := range args {
				svc.Install(ctx, id, alias, ide, hub.ArtifactType(artType))
			}

			wd, _ := os.Getwd()
			_ = hub.InstallRule(wd, ide)
			return nil
		},
	}

	cmd.Flags().StringVar(&alias, "alias", "", "Install under a custom name")
	cmd.Flags().StringVar(&artType, "type", "", "Artifact type (rule, agent, skill, command, mcp, ...)")
	registerArtifactTypeFlagCompletion(cmd)
	return cmd
}

func newHubUninstallCmd() *cobra.Command {
	var artType string

	cmd := &cobra.Command{
		Use:     "uninstall <artifact-id>",
		Aliases: []string{"remove"},
		Short:   "Uninstall an artifact from the current project",
		Args:    cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			for _, id := range args {
				svc.Uninstall(ctx, id, hub.ArtifactType(artType), ide)

				if artType == "knowledge" || artType == "" {
					wd, _ := os.Getwd()
					p := output.NewPrinter("hub:uninstall")
					knDir := filepath.Join(wd, brand.DotDir(), "knowledge", id)
					if _, statErr := os.Stat(knDir); statErr == nil {
						if rmErr := os.RemoveAll(knDir); rmErr == nil {
							p.Step("Knowledge context removed: %s", knDir)
						}
					}
					memDir := filepath.Join(wd, brand.DotDir(), "memory", id)
					if info, statErr := os.Lstat(memDir); statErr == nil {
						var rmErr error
						if info.Mode()&os.ModeSymlink != 0 {
							rmErr = os.Remove(memDir)
						} else {
							rmErr = os.RemoveAll(memDir)
						}
						if rmErr == nil {
							p.Step("Memory context removed: %s", memDir)
						}
					}
				}
			}

			wd, _ := os.Getwd()
			_ = hub.InstallRule(wd, ide)
			return nil
		},
	}

	cmd.Flags().StringVar(&artType, "type", "", "Artifact type (helps resolve ambiguous IDs)")
	registerArtifactTypeFlagCompletion(cmd)
	cmd.ValidArgsFunction = completionInstalledArtifactIDs()
	return cmd
}

func newHubUpdateCmd() *cobra.Command {
	var artType string
	cmd := &cobra.Command{
		Use:   "update [artifact-id]",
		Short: "Update installed hub artifacts to their latest versions",
		Long: `Update installed hub artifacts. Without arguments, updates all artifacts.
With an artifact ID, updates only that specific artifact.

Examples:
  graphit hub update                    # update all
  graphit hub update my-org/my-rule     # update a specific artifact
  graphit hub update my-rule --type rule`,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)

			if len(args) > 0 {
				svc.UpdateOneArtifact(ctx, args[0], hub.ArtifactType(artType), ide)
			} else {
				svc.Update(ctx, ide)
			}
			return nil
		},
	}
	cmd.Flags().StringVar(&artType, "type", "", "Artifact type (helps disambiguate)")
	return cmd
}

func newHubListCmd() *cobra.Command {
	var artType string

	cmd := &cobra.Command{
		Use:   "list",
		Short: "List available artifacts in the hub",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			svc.ListEntries(hub.ArtifactType(artType))
			return nil
		},
	}

	cmd.Flags().StringVar(&artType, "type", "", "Filter by artifact type")
	registerArtifactTypeFlagCompletion(cmd)
	return cmd
}

func newHubSearchCmd() *cobra.Command {
	var artType string

	cmd := &cobra.Command{
		Use:   "search <term>",
		Short: "Search for artifacts by name or description",
		Args:  cobra.MinimumNArgs(1),
		Example: `  ` + brand.BinName() + ` hub search oracle
  ` + brand.BinName() + ` hub search "payment gateway" --type knowledge`,
		RunE: func(cmd *cobra.Command, args []string) error {
			term := strings.Join(args, " ")
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			svc.SearchEntries(term, hub.ArtifactType(artType))
			return nil
		},
	}

	cmd.Flags().StringVar(&artType, "type", "", "Filter by artifact type (knowledge, ast, rule, skill, command, spec, ...)")
	registerArtifactTypeFlagCompletion(cmd)
	return cmd
}

func newHubShowCmd() *cobra.Command {
	var artType string

	cmd := &cobra.Command{
		Use:   "show <artifact-id>",
		Short: "Show details of a hub artifact",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			svc.ShowEntry(args[0], hub.ArtifactType(artType))
			return nil
		},
	}

	cmd.Flags().StringVar(&artType, "type", "", "Artifact type (helps resolve ambiguous IDs)")
	registerArtifactTypeFlagCompletion(cmd)
	cmd.ValidArgsFunction = completionInstalledArtifactIDs()
	return cmd
}

func newHubSubmitCmd() *cobra.Command {
	var version string
	var name string
	var description string
	var artType string
	var tags string

	cmd := &cobra.Command{
		Use:   "submit <artifact-id> <local-path>",
		Short: "Publish a local artifact to the hub",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			entryID := args[0]
			localPath := args[1]

			if _, err := os.Stat(localPath); err != nil {
				return err
			}

			ctx := context.Background()
			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			tagList := []string{}
			if tags != "" {
				for _, t := range strings.Split(tags, ",") {
					if t = strings.TrimSpace(t); t != "" {
						tagList = append(tagList, t)
					}
				}
			}

			meta := &hub.Entry{
				ID:          entryID,
				Name:        name,
				Type:        hub.ArtifactType(artType),
				Description: description,
				Tags:        tagList,
			}

			svc := hub.NewHubPresenter(reg)
			if version == "" {
				version = "1.0.0"
			}
			svc.Submit(ctx, entryID, localPath, meta, version)

			ide := resolveIDEFlag(cmd)
			hubSvc := hub.NewHubService(reg)
			if err := hubSvc.RecordPublish(ctx, entryID, hub.ArtifactType(artType), version, ide, ""); err != nil {
				p := output.NewPrinter("hub")
				p.StepWarn("Lockfile update: %v", err)
			}

			return nil
		},
	}

	cmd.Flags().StringVar(&version, "version", "1.0.0", "Artifact version")
	cmd.Flags().StringVar(&name, "name", "", "Display name")
	cmd.Flags().StringVar(&description, "description", "", "Description")
	cmd.Flags().StringVar(&artType, "type", "rule", "Artifact type (rule, agent, skill, command, mcp, power, ...)")
	cmd.Flags().StringVar(&tags, "tags", "", "Comma-separated tags")
	registerArtifactTypeFlagCompletion(cmd)
	return cmd
}

func newHubProjectsCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "projects",
		Short: "List registered projects",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()
			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}
			svc := hub.NewHubPresenter(reg)
			svc.ListProjects()
			return nil
		},
	}
}

func newHubTypePathCmd() *cobra.Command {
	binName := brand.BinName()

	cmd := &cobra.Command{
		Use:   "type-path <type> <name>",
		Short: "Print the IDE artifact path for a given type and name",
		Long: `Output the absolute path where an artifact should be created
for the current IDE and project.

For file-based types (rule, command, agent), returns the full file path
with the correct extension. For folder-based types (skill), returns the
directory path.

The output is a single clean path with no extra formatting.`,
		Args: cobra.ExactArgs(2),
		Example: `  ` + binName + ` hub type-path skill my-error-patterns
  ` + binName + ` hub type-path rule no-direct-db-access
  ` + binName + ` hub type-path command pre-deploy-check --ide cursor`,
		PersistentPreRunE: func(cmd *cobra.Command, args []string) error { return nil },
		RunE: func(cmd *cobra.Command, args []string) error {
			artType := strings.ToLower(args[0])
			artName := args[1]
			ide := resolveIDEFlag(cmd)

			wd, err := os.Getwd()
			if err != nil {
				return err
			}

			typePath, err := ideAdapter.ArtifactTypePath(wd, ide, artType, artName)
			if err != nil {
				return err
			}

			p := output.NewPrinter("")
			p.Data(typePath)
			return nil
		},
	}

	return cmd
}

func getRegistry(ctx context.Context) (*hub.RegistryManager, error) {
	reg, err := hub.NewRegistryManager(ctx)
	if err != nil {
		p := output.NewPrinter("hub")
		p.StepWarn("Hub registry unavailable — running in offline mode")

	}
	return reg, nil
}

func newHubLinkCmd() *cobra.Command {
	var artType string
	var sourcePath string

	cmd := &cobra.Command{
		Use:   "link <name>",
		Short: "Link a local project's artifacts into the current project via symlinks",
		Long: `Creates symlinks from the current project to a local source project,
bypassing the hub registry. Useful for local development and testing.

For AST:       .<brand>/ast/<name>       → <path>/.<brand>/ast/project
For Knowledge: .<brand>/knowledge/<name> → <path>/.<brand>/knowledge/project
For IDE types: <ide-dir>/<folder>/<name> → <path>/<ide-dir>/<folder>/<name>
MCP:           Not supported (requires actual IDE configuration)`,
		Args: cobra.ExactArgs(1),
		Example: `  ` + brand.BinName() + ` hub link my-project --path ../my-project --type knowledge
  ` + brand.BinName() + ` hub link my-project --path ../my-project --type ast
  ` + brand.BinName() + ` hub link error-patterns --path ../my-project --type skill`,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			svc.Link(ctx, args[0], sourcePath, ide, hub.ArtifactType(artType))

			wd, _ := os.Getwd()
			_ = hub.InstallRule(wd, ide)
			return nil
		},
	}

	cmd.Flags().StringVar(&sourcePath, "path", "", "Path to the source project (required)")
	cmd.Flags().StringVar(&artType, "type", "", "Artifact type: ast, knowledge, rule, skill, command, agent, mcp")
	_ = cmd.MarkFlagRequired("path")
	_ = cmd.MarkFlagRequired("type")
	registerArtifactTypeFlagCompletion(cmd)
	return cmd
}

func newHubUnlinkCmd() *cobra.Command {
	var artType string

	cmd := &cobra.Command{
		Use:   "unlink <name>",
		Short: "Remove a linked artifact from the current project",
		Args:  cobra.ExactArgs(1),
		Example: `  ` + brand.BinName() + ` hub unlink my-project --type knowledge
  ` + brand.BinName() + ` hub unlink error-patterns --type skill`,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			svc := hub.NewHubPresenter(reg)
			svc.Unlink(ctx, args[0], hub.ArtifactType(artType), ide)

			wd, _ := os.Getwd()
			_ = hub.InstallRule(wd, ide)
			return nil
		},
	}

	cmd.Flags().StringVar(&artType, "type", "", "Artifact type (required)")
	_ = cmd.MarkFlagRequired("type")
	registerArtifactTypeFlagCompletion(cmd)
	cmd.ValidArgsFunction = completionInstalledArtifactIDs()
	return cmd
}
