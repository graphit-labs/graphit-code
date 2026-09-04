package commands

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"sort"
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

	cmd.PersistentFlags().String("ide", "", "Target IDE (antigravity, cursor, claude, gemini, kiro, codex, opencode)")
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
		Use:   "install <artifact-id>[@version]",
		Short: "Install an artifact globally from the hub",
		Long: "Install an artifact into Graphit's global, version-keyed store. " +
			"The installed artifact can then be addressed as id@version without a project checkout.",
		Args: cobra.MinimumNArgs(1),
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

			p := output.NewPrinter("hub")
			svc := hub.NewHubService(reg)
			for _, id := range args {
				p.Running("Installing %s...", id)
				result, err := svc.Install(ctx, id, alias, ide, hub.ArtifactType(artType), "", "")
				if err != nil {
					p.Error("Install failed: %v", err)
					return err
				}
				p.Success("Installed %s (%s) @%s", result.Name, result.ArtType, result.Version)
			}

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
		Short:   "Uninstall a globally installed artifact",
		Args:    cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			p := output.NewPrinter("hub")
			svc := hub.NewHubService(reg)
			for _, id := range args {
				p.Running("Removing %s...", id)
				if err := svc.Uninstall(ctx, id, hub.ArtifactType(artType), true, ide, ""); err != nil {
					p.Error("Uninstall failed: %v", err)
					return err
				}
				p.Success("Removed %s", id)

			}

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

			p := output.NewPrinter("hub")
			svc := hub.NewHubService(reg)

			if len(args) > 0 {
				id := args[0]
				p.Running("Updating %s...", id)
				if err := svc.UpdateOne(ctx, id, hub.ArtifactType(artType), ide, ""); err != nil {
					p.Error("Update failed: %v", err)
					return err
				}
				p.Success("Updated %s", id)
			} else {
				p.Running("Checking for updates...")
				results := svc.UpdateAll(ctx, ide, "")
				if len(results) == 0 {
					p.Info("All artifacts are up to date.")
					return nil
				}

				updated := 0
				for artID, err := range results {
					if err != nil {
						p.StepWarn("Update failed for %q: %v", artID, err)
					} else {
						p.StepOK("Updated %s", artID)
						updated++
					}
				}

				if updated > 0 {
					p.Success("Updated %d artifact(s).", updated)
				} else {
					p.Info("All artifacts are up to date.")
				}
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

			p := output.NewPrinter("hub")
			entries := reg.ListEntries(hub.ArtifactType(artType))
			if len(entries) == 0 {
				p.Info("No entries found.")
				return nil
			}

			sort.Slice(entries, func(i, j int) bool {
				if entries[i].Type != entries[j].Type {
					return entries[i].Type < entries[j].Type
				}
				return entries[i].ID < entries[j].ID
			})

			rows := make([][2]string, 0, len(entries))
			for _, e := range entries {
				name := e.Name
				if name == "" {
					name = e.ID
				}
				version := e.Latest
				if version == "" {
					version = "—"
				}
				rows = append(rows, [2]string{
					fmt.Sprintf("%-12s  %s", string(e.Type), name),
					fmt.Sprintf("%s  (%s)", e.ID, version),
				})
			}

			p.Table([2]string{"TYPE / NAME", "ID / VERSION"}, rows)
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

			p := output.NewPrinter("hub")
			entries := reg.SearchEntries(term, hub.ArtifactType(artType))
			if len(entries) == 0 {
				p.Info("No results for %q.", term)
				return nil
			}

			rows := make([][2]string, 0, len(entries))
			for _, e := range entries {
				name := e.Name
				if name == "" {
					name = e.ID
				}
				desc := e.Description
				if len(desc) > 60 {
					desc = desc[:57] + "..."
				}
				if desc == "" {
					desc = "—"
				}
				version := e.Latest
				if version == "" {
					version = "—"
				}
				rows = append(rows, [2]string{
					fmt.Sprintf("%-12s  %s", string(e.Type), name),
					fmt.Sprintf("%s  (%s)  %s", e.ID, version, desc),
				})
			}

			p.Table([2]string{"TYPE / NAME", "ID / VERSION / DESCRIPTION"}, rows)
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

			p := output.NewPrinter("hub")
			entry := reg.GetEntry(args[0], hub.ArtifactType(artType))
			if entry == nil {
				p.Error("Entry %q not found in registry.", args[0])
				return nil
			}

			p.Header("Artifact: %s", entry.Name)
			p.KeyValue("ID", entry.ID)
			p.KeyValue("Type", string(entry.Type))
			p.KeyValue("Latest", entry.Latest)
			if len(entry.Versions) > 0 {
				p.Step("Available versions:")
				for _, v := range entry.Versions {
					p.ListItem("%s", v)
				}
			}
			p.KeyValue("Description", entry.Description)
			if len(entry.Tags) > 0 {
				joinStrings := func(ss []string) string {
					result := ""
					for i, s := range ss {
						if i > 0 {
							result += ", "
						}
						result += s
					}
					return result
				}
				p.KeyValue("Tags", joinStrings(entry.Tags))
			}
			if len(entry.Dependencies) > 0 {
				p.Step("Dependencies:")
				for _, dep := range entry.Dependencies {
					p.ListItem("%s (%s)", dep.ID, dep.Type)
				}
			}
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

			p := output.NewPrinter("hub")
			if version == "" {
				version = "1.0.0"
			}
			p.Running("Publishing %s@%s to hub...", entryID, version)
			p.Step("Zipping artifact files")

			if err := reg.PublishEntry(ctx, entryID, localPath, meta, version); err != nil {
				p.Error("Publish failed: %v", err)
				return err
			}
			p.Success("Published %s@%s", entryID, version)

			ide := resolveIDEFlag(cmd)
			hubSvc := hub.NewHubService(reg)
			if err := hubSvc.RecordPublish(ctx, entryID, hub.ArtifactType(artType), version, ide, ""); err != nil {
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

			p := output.NewPrinter("hub")
			projects := reg.ListProjects()
			if len(projects) == 0 {
				p.Info("No projects registered.")
				return nil
			}

			rows := make([][2]string, 0, len(projects))
			for _, proj := range projects {
				rows = append(rows, [2]string{proj.Name, proj.RemoteID})
			}

			p.Table([2]string{"PROJECT", "REMOTE ID"}, rows)
			return nil
		},
	}
}

func newHubTypePathCmd() *cobra.Command {
	binName := brand.BinName()

	cmd := &cobra.Command{
		Use:   "type-path <type> <name>",
		Short: "Print the IDE path for a physical artifact",
		Long: `Output the absolute path where an artifact should be created
for the current IDE and project.

For file-based types (command and agent), returns the full file path with
the correct extension. For folder-based types (skill), returns the directory
path. Rules have no IDE path because lifecycle hooks load them dynamically.

The output is a single clean path with no extra formatting.`,
		Args: cobra.ExactArgs(2),
		Example: `  ` + binName + ` hub type-path skill my-error-patterns
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
		Short: "Link a local artifact into the current project",
		Long: `Links the current project to a local source, bypassing the Hub registry.
Useful for local development and testing. Rules are read dynamically by lifecycle
hooks from a directory containing RULE.md; they are never copied into an IDE.

AST/Knowledge: record a pointer to the source project's compiled global store
Rule:          read <path>/RULE.md through dynamic hook context
IDE types:     link the adapter-native target to the source artifact
MCP:           synchronize the source definition into the active adapter`,
		Args: cobra.ExactArgs(1),
		Example: `  ` + brand.BinName() + ` hub link my-project --path ../my-project --type knowledge
  ` + brand.BinName() + ` hub link my-project --path ../my-project --type ast
  ` + brand.BinName() + ` hub link review-policy --path ../review-policy --type rule
  ` + brand.BinName() + ` hub link error-patterns --path ../my-project --type skill`,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()

			reg, err := getRegistry(ctx)
			if err != nil {
				return err
			}

			p := output.NewPrinter("hub")
			svc := hub.NewHubService(reg)
			p.Running("Linking %s from %s...", args[0], sourcePath)

			result, err := svc.Link(ctx, args[0], sourcePath, ide, hub.ArtifactType(artType), "")
			if err != nil {
				p.Error("Link failed: %v", err)
				return err
			}

			for _, link := range result.Links {
				p.Step("%s", link)
			}
			p.Success("Linked %s (%s)", result.ArtifactID, result.ArtType)

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

			p := output.NewPrinter("hub")
			svc := hub.NewHubService(reg)
			p.Running("Unlinking %s...", args[0])

			if err := svc.Unlink(ctx, args[0], ide, hub.ArtifactType(artType), ""); err != nil {
				p.Error("Unlink failed: %v", err)
				return err
			}
			p.Success("Unlinked %s", args[0])

			return nil
		},
	}

	cmd.Flags().StringVar(&artType, "type", "", "Artifact type (required)")
	_ = cmd.MarkFlagRequired("type")
	registerArtifactTypeFlagCompletion(cmd)
	cmd.ValidArgsFunction = completionInstalledArtifactIDs()
	return cmd
}
