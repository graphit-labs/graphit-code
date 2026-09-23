package commands

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/tray"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/spf13/cobra"
)

var rootCmd = &cobra.Command{
	Use:           brand.BinName(),
	Short:         brand.DisplayName,
	Long:          brand.DisplayName,
	SilenceUsage:  true,
	SilenceErrors: true,
	Version:       version.Version,
	PersistentPreRunE: func(cmd *cobra.Command, args []string) error {

		name := cmd.Name()
		if name == "daemon" || name == "tray" || name == "setup" || name == "uninstall" || name == "self-update" || name == "provider" || name == "login" || name == "logout" || name == "account" || name == "_internal" || name == "_session-hook" {
			return nil
		}

		for p := cmd.Parent(); p != nil; p = p.Parent() {
			if p.Name() == "daemon" || p.Name() == "tray" || p.Name() == "provider" || p.Name() == "account" || p.Name() == "_internal" {
				return nil
			}
		}

		if config.IsModuleDisabled("daemon", nil, nil) {
			return nil
		}
		_, err := daemon.EnsureRunning()
		if err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Graphit daemon service warning: %v\n", err)
		}
		_ = tray.EnsureRunning()
		return nil
	},
}

func init() {

	rootCmd.SetHelpCommand(&cobra.Command{Hidden: true})

	rootCmd.AddCommand(
		newSetupCmd(),
		newInitCmd(),
		newUpdateCmd(),
		newSyncCmd(),
		newRemoveCmd(),
		newUninstallCmd(),
		newConfigCmd(),
		newSelfUpdateCmd(),
		newUICmd(),
		newHubCmd(),
		newASTCmd(),
		newKnowledgeCmd(),
		newMemoryCmd(),
		newTaskCmd(),
		newWikiCmd(),
		newLiveCmd(),
		newDreamCmd(),
		newDaemonCmd(),
		newTrayCmd(),
		newMCPCmd(),
		newProviderCmd(),
		newLoginCmd(),
		newLogoutCmd(),
		newAccountCmd(),
		newClusterCmd(),
		newSessionHookCmd(),
	)

	rootCmd.PersistentFlags().BoolP("verbose", "v", false, "Enable verbose output")
	rootCmd.PersistentFlags().BoolVar(&nonInteractive, "non-interactive", false, "Never prompt, open a browser, editor, or pager; fail when input is missing")
	rootCmd.PersistentFlags().StringArrayP("config", "c", nil, "Override runtime config key=value (repeatable, e.g. -c agent=cursor)")
}

func Execute() {
	slogutil.InitFileLogger(brand.GlobalDir())
	defer slogutil.CloseFileLogger()

	err := rootCmd.Execute()

	hub.WaitForPendingEvents()

	if err != nil {
		output.Fatal("%s", err)
	}
}

var errNotManaged = "this project is not managed by " + brand.DisplayName + " — run '" + brand.BinName() + " init' first"
var errNotSetup = "framework not configured — run '" + brand.BinName() + " setup' first"

func requireProject(_ *cobra.Command, _ []string) error {
	lp := lockfilePath()
	if _, err := os.Stat(lp); os.IsNotExist(err) {
		return errors.New(errNotManaged)
	}
	return nil
}

func requireSetup(cmd *cobra.Command, _ []string) error {
	if !config.IsSetupDone() {
		return errors.New(errNotSetup)
	}
	return nil
}

func requireSetupAndProject(cmd *cobra.Command, args []string) error {
	if err := requireSetup(cmd, args); err != nil {
		return err
	}
	return requireProject(cmd, args)
}

func resolveAgentFlag(cmd *cobra.Command) string {
	flagVal, _ := cmd.Flags().GetString("agent")
	inlineCfg := parseInlineConfig(cmd)
	projectCfg, lockfileAgents := loadProjectLockInfo()
	return config.ResolveProjectAgent(flagVal, inlineCfg, projectCfg, lockfileAgents)
}

func parseInlineConfig(cmd *cobra.Command) config.ConfigMap {
	pairs, _ := cmd.Flags().GetStringArray("config")
	if len(pairs) == 0 {
		return nil
	}
	cfg := make(config.ConfigMap)
	for _, pair := range pairs {
		parts := strings.SplitN(pair, "=", 2)
		if len(parts) == 2 && parts[0] != "" {
			config.SetConfigValue(cfg, parts[0], parts[1])
		}
	}
	return cfg
}

func loadProjectConfig() config.ConfigMap {
	lp := lockfilePath()
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config
	}
	return nil
}

func loadProjectLockInfo() (config.ConfigMap, []string) {
	lp := lockfilePath()
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config, lf.Agents
	}
	return nil, nil
}

func loadProjectLockInfoFromDir(dir string) (config.ConfigMap, []string) {
	lp := filepath.Join(dir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config, lf.Agents
	}
	return nil, nil
}

func loadProjectConfigFromDir(dir string) config.ConfigMap {
	lp := filepath.Join(dir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config
	}
	return nil
}
