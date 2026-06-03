package commands

import (
	"errors"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
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
	PersistentPreRun: func(cmd *cobra.Command, args []string) {

		name := cmd.Name()
		if name == "daemon" || name == "setup" || name == "uninstall" || name == "_internal" {
			return
		}

		for p := cmd.Parent(); p != nil; p = p.Parent() {
			if p.Name() == "daemon" || p.Name() == "_internal" {
				return
			}
		}

		if config.IsModuleDisabled("daemon", nil, nil) {
			return
		}
		_, _ = daemon.EnsureRunning()
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
		newWikiCmd(),
		newDreamCmd(),
		newImprovementsCmd(),
		newDaemonCmd(),
		newMCPCmd(),
		newClusterCmd(),
	)

	rootCmd.PersistentFlags().BoolP("verbose", "v", false, "Enable verbose output")
	rootCmd.PersistentFlags().StringArrayP("config", "c", nil, "Override config key=value (repeatable, e.g. -c ide=cursor -c hub.repo=git@...)")
}

func Execute() {
	slogutil.InitFileLogger(brand.GlobalDir())
	defer slogutil.CloseFileLogger()

	err := rootCmd.Execute()

	memory.WaitForPendingPushes()

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

func resolveIDEFlag(cmd *cobra.Command) string {
	flagVal, _ := cmd.Flags().GetString("ide")
	inlineCfg := parseInlineConfig(cmd)
	projectCfg, lockfileIDEs := loadProjectLockInfo()
	return config.ResolveProjectIDE(flagVal, inlineCfg, projectCfg, lockfileIDEs)
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
		return lf.Config, lf.IDEs
	}
	return nil, nil
}

func loadProjectLockInfoFromDir(dir string) (config.ConfigMap, []string) {
	lp := filepath.Join(dir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		return lf.Config, lf.IDEs
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
