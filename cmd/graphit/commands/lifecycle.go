package commands

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"golang.org/x/term"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/updater"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/spf13/cobra"
)

func installAllRules(p *output.Printer, wd, ideName string) {
	projectCfg := loadProjectConfigFromDir(wd)
	removeRetiredImprovementsGuidance(p, wd, ideName)

	for _, r := range []struct {
		name    string
		install func(string, string) error
		remove  func(string, string) error
		skill   func(string, string) error
	}{
		{"Knowledge", knowledge.InstallRule, knowledge.RemoveRule, knowledge.InstallSkill},
		{"AST", ast.InstallRule, ast.RemoveRule, ast.InstallSkill},
		{"Hub", hub.InstallRule, hub.RemoveRule, hub.InstallSkill},
		{"Memory", memory.InstallRule, memory.RemoveRule, memory.InstallSkill},
	} {
		moduleLower := strings.ToLower(r.name)
		if config.IsModuleDisabled(moduleLower, nil, projectCfg) {
			if err := r.remove(wd, ideName); err != nil {
				p.StepWarn("%s rule removal: %v", r.name, err)
			}
		} else {
			if err := r.install(wd, ideName); err != nil {
				p.StepWarn("%s rule: %v", r.name, err)
			}
		}

		if err := r.skill(wd, ideName); err != nil {
			p.StepWarn("%s skill: %v", r.name, err)
		}
	}
}

func removeAllRules(p *output.Printer, wd, ide string) {
	for _, r := range []struct {
		name        string
		removeRule  func(string, string) error
		removeSkill func(string, string) error
	}{
		{"Knowledge", knowledge.RemoveRule, knowledge.RemoveSkill},
		{"AST", ast.RemoveRule, ast.RemoveSkill},
		{"Hub", hub.RemoveRule, hub.RemoveSkill},
		{"Memory", memory.RemoveRule, memory.RemoveSkill},
	} {
		if err := r.removeRule(wd, ide); err != nil {
			p.StepWarn("%s rule cleanup: %v", r.name, err)
		}
		if err := r.removeSkill(wd, ide); err != nil {
			p.StepWarn("%s skill cleanup: %v", r.name, err)
		}
	}
	removeRetiredImprovementsGuidance(p, wd, ide)
}

func removeRetiredImprovementsGuidance(p *output.Printer, projectDir, ideName string) {
	if err := ide.RemoveMandateTrigger(projectDir, ideName, "imp_rule"); err != nil {
		p.StepWarn("retired Improvements mandate cleanup: %v", err)
	}
	if err := ide.RemoveManagedSkill(projectDir, ideName, brand.SkillDirName("improvements")); err != nil {
		p.StepWarn("retired Improvements skill cleanup: %v", err)
	}
}

func newInitCmd() *cobra.Command {
	var flagID string
	var flagName string
	var flagDesc string

	cmd := &cobra.Command{
		Use:   "init",
		Short: "Initialize " + brand.DisplayName + " in the current project",
		Long: `Initialize ` + brand.DisplayName + ` for the current project.

This command:
  • Creates a project identity and ` + brand.LockFileName() + `
  • Installs baseline artifacts (rules, commands, agents)
  • Configures the selected IDE adapter

Use --id, --name and --description to set the project identity inline (useful
for CI/CD or scripted setups). When provided, these flags take precedence over
auto-generated values and interactive prompts.`,
		PreRunE: requireSetup,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()
			p := output.NewPrinter("")

			p.Running("Initializing project...")

			wd, _ := os.Getwd()
			lockPath := filepath.Join(wd, brand.LockFileName())
			lf, _ := hub.LoadLockfile(lockPath)

			if flagDesc != "" {
				if lf == nil {
					lf = &hub.Lockfile{Artifacts: make(map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta)}
				}
				lf.Project.Description = flagDesc
				if err := hub.SaveLockfile(lockPath, lf); err != nil {
					p.StepWarn("Could not save description: %v", err)
				} else {
					p.StepOK("Description set: %s", flagDesc)
				}
			} else if lf == nil || lf.Project.Description == "" {
				var currentDesc string
				if lf != nil {
					currentDesc = lf.Project.Description
				}
				if currentDesc != "" {
					p.Detail("Current description", currentDesc)
				}
				fmt.Print("  Enter project description [leave blank to skip]: ")
				reader := bufio.NewReader(os.Stdin)
				descInput, _ := reader.ReadString('\n')
				descInput = strings.TrimSpace(descInput)
				if descInput != "" {

					if lf == nil {
						lf = &hub.Lockfile{Artifacts: make(map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta)}
					}
					lf.Project.Description = descInput
					if err := hub.SaveLockfile(lockPath, lf); err != nil {
						p.StepWarn("Could not save description: %v", err)
					} else {
						p.StepOK("Description set: %s", descInput)
					}
				}
			}

			if flagID != "" {
				if lf == nil {
					lf = &hub.Lockfile{Artifacts: make(map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta)}
				}
				lf.Project.ID = flagID
				p.StepOK("Project ID set: %s", flagID)
			}

			if flagName != "" {
				if lf == nil {
					lf = &hub.Lockfile{Artifacts: make(map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta)}
				}
				lf.Project.Name = flagName
				p.StepOK("Project name set: %s", flagName)
			}

			if flagID != "" || flagName != "" {
				if err := hub.SaveLockfile(lockPath, lf); err != nil {
					p.StepWarn("Could not save project identity: %v", err)
				}
			}

			reg, err := hub.NewRegistryManager(ctx)
			if err != nil {
				p.StepWarn("Hub registry unavailable (offline mode): %v", err)

				reg, _ = hub.NewRegistryManager(ctx)
			}

			if err := hub.OnInit(ctx, reg, ide, ""); err != nil {
				p.Error("%v", err)
				return err
			}

			gitignorePath := filepath.Join(wd, ".gitignore")
			if err := git.InjectGitignore(gitignorePath, brand.GitignoreContent()); err != nil {
				p.StepWarn(".gitignore: %v", err)
			}

			wd, _ = os.Getwd()
			_ = memory.EnsureScopeDirs("project", wd)
			_ = memory.EnsureScopeDirs("user", wd)

			p.Success("Project initialized successfully")

			p.Running("Synchronizing project...")
			runSyncPhase1(ctx, wd, []string{ide}, p)
			spawnBackgroundSync(wd, ide)

			return nil
		},
	}
	cmd.Flags().String("ide", "", "Target IDE (antigravity, cursor, claude, gemini, kiro, codex, opencode)")
	registerIDEFlagCompletion(cmd)
	cmd.Flags().StringVar(&flagID, "id", "", "Project ULID (overrides auto-generated ID)")
	cmd.Flags().StringVar(&flagName, "name", "", "Project name (overrides auto-detected name)")
	cmd.Flags().StringVar(&flagDesc, "description", "", "Project description (skips interactive prompt)")
	return cmd
}

func newUpdateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "update",
		Short:   "Update all installed artifacts to their latest versions",
		PreRunE: requireSetupAndProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()
			p := output.NewPrinter("")

			p.Running("Checking for updates...")

			reg, err := hub.NewRegistryManager(ctx)
			if err != nil {
				p.Error("Hub registry unavailable: %v", err)
				return err
			}

			if err := hub.OnUpdate(ctx, reg, ide, ""); err != nil {
				p.Error("%v", err)
				return err
			}

			wd, _ := os.Getwd()
			installAllRules(p, wd, ide)

			p.Success("Update complete")
			return nil
		},
	}
	cmd.Flags().String("ide", "", "Target IDE (antigravity, cursor, claude, gemini, kiro, codex, opencode)")
	registerIDEFlagCompletion(cmd)
	return cmd
}

func newRemoveCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "remove",
		Short:   "Remove " + brand.DisplayName + " from the current project",
		PreRunE: requireSetupAndProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			ide := resolveIDEFlag(cmd)
			ctx := context.Background()
			p := output.NewPrinter("")

			p.Running("Removing %s from this project...", brand.DisplayName)

			wd, _ := os.Getwd()

			hm := git.NewHookManager("")
			if err := hm.Remove(); err != nil {
				p.StepWarn("Git hooks cleanup: %v", err)
			}

			if _, err := git.RemoveGitignore(filepath.Join(wd, ".gitignore")); err != nil {
				p.StepWarn(".gitignore cleanup: %v", err)
			}

			reg, _ := hub.NewRegistryManager(ctx)

			if err := hub.OnRemove(ctx, reg, ide, ""); err != nil {
				p.Error("%v", err)
				return err
			}

			removeAllRules(p, wd, ide)

			p.Success("%s removed from this project", brand.DisplayName)
			return nil
		},
	}
	cmd.Flags().String("ide", "", "Target IDE (antigravity, cursor, claude, gemini, kiro, codex, opencode)")
	registerIDEFlagCompletion(cmd)
	return cmd
}

func newConfigCmd() *cobra.Command {
	var global bool
	var get bool
	var unset bool
	var list bool
	var secret bool

	cmd := &cobra.Command{
		Use:   "config [--global] [--get|--unset|--list|--secret] [key] [value]",
		Short: "Manage " + brand.DisplayName + " configuration",
		Long: `Manage ` + brand.DisplayName + ` configuration (per-project or global).

Per-project config is stored in ` + brand.LockFileName() + ` (default).
Global config is stored in ~/` + brand.DotDir() + `/config.json (use --global).

Examples:
  ` + brand.BinName() + ` config ide cursor                   # set per-project
  ` + brand.BinName() + ` config --global ide cursor          # set global
  ` + brand.BinName() + ` config --get ide                    # get per-project
  ` + brand.BinName() + ` config --get --global ide           # get global
  ` + brand.BinName() + ` config --unset ide                  # unset per-project
  ` + brand.BinName() + ` config --global --unset ide         # unset global
  ` + brand.BinName() + ` config --list                       # list per-project config
  ` + brand.BinName() + ` config --list --global              # list global config
  ` + brand.BinName() + ` config --secret openai.key < key.txt # set secret value from stdin`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			if !global {
				if err := requireProject(cmd, args); err != nil {
					return err
				}
			}

			if list {
				return runConfigList(p, global)
			}

			if get {
				if len(args) != 1 {
					return fmt.Errorf("usage: %s config --get [--global] <key>", brand.BinName())
				}
				return runConfigGet(p, args[0], global)
			}

			if unset {
				if len(args) != 1 {
					return fmt.Errorf("usage: %s config [--global] --unset <key>", brand.BinName())
				}
				return runConfigUnset(p, args[0], global)
			}

			if secret {
				if len(args) != 1 {
					return fmt.Errorf("usage: %s config [--global] --secret <key>", brand.BinName())
				}
				var bytes []byte
				var err error

				if term.IsTerminal(int(os.Stdin.Fd())) {
					fmt.Printf("  Enter secret value for %s: ", args[0])
					bytes, err = term.ReadPassword(int(os.Stdin.Fd()))
					p.Blank()
				} else {
					bytes, err = io.ReadAll(os.Stdin)
				}

				if err != nil {
					return fmt.Errorf("reading secret: %w", err)
				}
				value := strings.TrimSpace(string(bytes))
				if value == "" {
					return fmt.Errorf("empty secret received")
				}
				return runConfigSet(p, args[0], value, global, true)
			}

			if len(args) != 2 {
				return fmt.Errorf("usage: %s config [--global] <key> <value>", brand.BinName())
			}
			return runConfigSet(p, args[0], args[1], global, false)
		},
	}

	cmd.Flags().BoolVar(&global, "global", false, "Use global config (~/"+brand.DotDir()+"/config.json)")
	cmd.Flags().BoolVar(&get, "get", false, "Get a configuration value")
	cmd.Flags().BoolVar(&unset, "unset", false, "Unset a configuration key")
	cmd.Flags().BoolVar(&list, "list", false, "List all configuration")
	cmd.Flags().BoolVar(&secret, "secret", false, "Read configuration value from stdin (useful for secrets)")

	return cmd
}

func runConfigSet(p *output.Printer, key, value string, global bool, isSecret bool) error {
	displayValue := value
	if isSecret {
		displayValue = "***"
	}

	if global {
		if err := config.SetGlobalConfigValue(key, value); err != nil {
			return err
		}
		p.Success("Set %s = %s (global)", key, displayValue)
		return nil
	}

	lockPath := lockfilePath()
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil {
		return fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return fmt.Errorf("no project found — run '%s init' first", brand.BinName())
	}
	if lf.Config == nil {
		lf.Config = make(map[string]any)
	}
	config.SetConfigValue(lf.Config, key, value)
	if err := hub.SaveLockfile(lockPath, lf); err != nil {
		return fmt.Errorf("saving lockfile: %w", err)
	}
	p.Success("Set %s = %s (project)", key, displayValue)
	return nil
}

func runConfigGet(p *output.Printer, key string, global bool) error {
	if global {
		val, ok, err := config.GetGlobalConfigValue(key)
		if err != nil {
			return err
		}
		if !ok {
			return fmt.Errorf("key %q not set in global config", key)
		}
		if config.IsSecretConfigKey(key) {
			val = "***"
		}
		p.Data(val)
		return nil
	}

	lockPath := lockfilePath()
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil {
		return fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return fmt.Errorf("no project found — run '%s init' first", brand.BinName())
	}
	if lf.Config == nil {
		return fmt.Errorf("key %q not set in project config", key)
	}
	val, ok := config.GetConfigValue(lf.Config, key)
	if !ok {
		return fmt.Errorf("key %q not set in project config", key)
	}
	if config.IsSecretConfigKey(key) {
		val = "***"
	}
	p.Data(val)
	return nil
}

func runConfigUnset(p *output.Printer, key string, global bool) error {
	if global {
		if err := config.UnsetGlobalConfigValue(key); err != nil {
			return err
		}
		p.Success("Unset %s (global)", key)
		return nil
	}

	lockPath := lockfilePath()
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil {
		return fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return fmt.Errorf("no project found — run '%s init' first", brand.BinName())
	}
	if lf.Config != nil {
		config.UnsetConfigValue(lf.Config, key)
	}
	if err := hub.SaveLockfile(lockPath, lf); err != nil {
		return fmt.Errorf("saving lockfile: %w", err)
	}
	p.Success("Unset %s (project)", key)
	return nil
}

func runConfigList(p *output.Printer, global bool) error {
	if global {
		cfg, err := config.LoadGlobalConfig()
		if err != nil {
			return err
		}
		entries := config.ListConfigEntries(cfg)
		if len(entries) == 0 {
			p.Info("No global configuration set.")
			return nil
		}
		for _, e := range entries {
			value := e[1]
			if config.IsSecretConfigKey(e[0]) {
				value = "***"
			}
			p.KeyValue(e[0], value)
		}
		return nil
	}

	lockPath := lockfilePath()
	lf, err := hub.LoadLockfile(lockPath)
	if err != nil {
		return fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return fmt.Errorf("no project found — run '%s init' first", brand.BinName())
	}
	if len(lf.Config) == 0 {
		p.Info("No project configuration set.")
		return nil
	}
	entries := config.ListConfigEntries(lf.Config)
	for _, e := range entries {
		value := e[1]
		if config.IsSecretConfigKey(e[0]) {
			value = "***"
		}
		p.KeyValue(e[0], value)
	}
	return nil
}

func lockfilePath() string {
	wd, _ := os.Getwd()
	return filepath.Join(wd, brand.LockFileName())
}

func newSelfUpdateCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "self-update",
		Short: "Update the " + brand.BinName() + " binary to the latest version",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			task := p.StartTask("Checking for updates...")

			if brand.GitHubRepo == "" && brand.SelfUpdateURL == "" {
				task.Fail("No update source configured")
				return fmt.Errorf("self-update is not configured for this build — contact your distributor")
			}

			currentExe, err := os.Executable()
			if err != nil {
				task.Fail("Cannot determine current executable: %v", err)
				return fmt.Errorf("cannot determine current executable: %w", err)
			}
			currentExe, _ = filepath.EvalSymlinks(currentExe)

			launcherPath := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
			if launcherPath != "" {
				currentExe = launcherPath
			}

			task.Update("Fetching latest release...")
			release, err := updater.LatestRelease(brand.GitHubRepo, brand.SelfUpdateURL)
			if err != nil {
				task.Fail("Failed to fetch latest release: %v", err)
				return fmt.Errorf("fetching latest release: %w", err)
			}

			if !updater.NeedsUpdate(version.Version, release.TagName) {
				task.Done("Already up to date (%s)", version.Version)
				return nil
			}

			task.Update("Updating %s → %s...", version.Version, release.TagName)

			archiveName := updater.PlatformArchiveName(brand.BinName())
			archiveURL := updater.FindAsset(release, archiveName)
			if archiveURL == "" {
				task.Fail("No archive available for this platform (%s)", archiveName)
				return fmt.Errorf("no release asset %q found in %s", archiveName, release.TagName)
			}
			checksumURL := strings.TrimSuffix(archiveURL, ".tar.gz") + ".sha256"

			tmpDir := filepath.Dir(currentExe)
			tmpArchive, err := os.CreateTemp(tmpDir, "."+brand.Brand+"-update-archive-*")
			if err != nil {
				tmpArchive, err = os.CreateTemp("", brand.Brand+"-update-archive-*")
				if err != nil {
					task.Fail("Create temp archive: %v", err)
					return fmt.Errorf("create temp archive: %w", err)
				}
			}
			tmpArchivePath := tmpArchive.Name()
			_ = tmpArchive.Close()
			defer func() { _ = os.Remove(tmpArchivePath) }()

			task.Update("Downloading %s...", archiveName)
			if err := updater.Download(archiveURL, tmpArchivePath, nil); err != nil {
				task.Fail("Download failed: %v", err)
				return fmt.Errorf("downloading archive: %w", err)
			}

			checksumTmp, err := os.CreateTemp("", brand.Brand+"-checksum-*")
			if err != nil {
				task.Fail("Create checksum temp file: %v", err)
				return fmt.Errorf("create checksum temp file: %w", err)
			}
			checksumTmpPath := checksumTmp.Name()
			_ = checksumTmp.Close()
			defer func() { _ = os.Remove(checksumTmpPath) }()

			if err := updater.Download(checksumURL, checksumTmpPath, nil); err != nil {
				task.Fail("Download checksum failed: %v", err)
				return fmt.Errorf("downloading checksum: %w", err)
			}

			task.Update("Verifying checksum...")
			if err := updater.VerifyChecksum(tmpArchivePath, checksumTmpPath); err != nil {
				task.Fail("Checksum verification failed: %v", err)
				return fmt.Errorf("checksum verification: %w", err)
			}

			tmpBin, err := os.CreateTemp(tmpDir, "."+brand.Brand+"-update-bin-*")
			if err != nil {
				tmpBin, err = os.CreateTemp("", brand.Brand+"-update-bin-*")
				if err != nil {
					task.Fail("Create temp binary: %v", err)
					return fmt.Errorf("create temp binary: %w", err)
				}
			}
			tmpBinPath := tmpBin.Name()
			_ = tmpBin.Close()
			defer func() { _ = os.Remove(tmpBinPath) }()

			task.Update("Extracting archive...")
			if err := updater.ExtractFromTarGz(tmpArchivePath, brand.BinName(), tmpBinPath); err != nil {
				task.Fail("Extraction failed: %v", err)
				return fmt.Errorf("extracting binary: %w", err)
			}

			if err := os.Chmod(tmpBinPath, 0o755); err != nil {
				task.Fail("Chmod: %v", err)
				return fmt.Errorf("chmod: %w", err)
			}

			if err := updater.AtomicReplace(tmpBinPath, currentExe); err != nil {
				task.Fail("Replace binary: %v", err)
				return fmt.Errorf("replacing binary: %w", err)
			}

			task.Done("Updated to %s", release.TagName)

			if daemon.IsSchedulerInstalled() {
				schedTask := p.StartTask("Updating OS scheduler...")
				if err := daemon.InstallScheduler(); err != nil {
					schedTask.Fail("Scheduler update: %v", err)
				} else {
					schedTask.Done("OS scheduler updated")
				}
			}

			return nil
		},
	}
}

func newSyncCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "sync",
		Short: "Synchronize the entire project state",
		Long: `Force a full synchronization of the project state.

Phase 1 (synchronous):
  • Pull latest hub repository content
  • Pull latest memory repository content
  • Re-install all IDE rule blocks
  • Sync the IDE adapter with the current lockfile
  • Sync git hooks
  • Reindex the AST knowledge graph
  • Reindex the docs/knowledge wiki
  • Reindex memory wikis

Phase 2 (background by default):
  • Generate vector embeddings for semantic search
  • Run memory GC

Flags:
  --no-background   Run both phases in the same process with terminal output
  --heavy           Run only Phase 2 with terminal output
  --debounce        Skip when a sync already finished this recently

Designed to be run as fire-and-forget: ` + brand.BinName() + ` sync &`,
		PreRunE: requireSetupAndProject,
		RunE: func(cmd *cobra.Command, args []string) error {
			wd, _ := os.Getwd()

			if heavy, _ := cmd.Flags().GetBool("heavy"); heavy {
				// Phase 2 saturates the machine: embeddings drive an ONNX session
				// sized to the whole CPU budget, and the memory GC that follows
				// reindexes both wikis. A second copy does not halve the work, it
				// doubles the contention — so the loser exits rather than queueing.
				// Everything it would have done, the winner is already doing.
				lock, proceed := acquireSyncLock(wd, "sync-heavy.lock")
				if !proceed {
					return nil
				}
				defer lock.Release()

				p := output.NewPrinter("")
				runSyncHeavyTasks(cmd.Context(), wd, p)
				return nil
			}

			p := output.NewPrinter("")
			ctx := context.Background()

			// The git hooks fire this on post-commit, pre-push and post-merge, so a
			// commit followed by a push used to pay for two full reindexes of a tree
			// that changed once. The window collapses the burst into one sync.
			debounce, _ := cmd.Flags().GetDuration("debounce")
			if syncedWithin(wd, debounce) {
				return nil
			}

			lock, proceed := acquireSyncLock(wd, "sync.lock")
			if !proceed {
				// Hook-driven runs are silent by contract; an interactive one says
				// why it did nothing.
				if debounce == 0 {
					p.StepWarn("Another sync is already running — skipping")
				}
				return nil
			}
			defer lock.Release()

			p.Running("Synchronizing project...")

			explicitIDE, _ := cmd.Flags().GetString("ide")
			lf, lfErr := hub.LoadLockfile(filepath.Join(wd, brand.LockFileName()))
			var idesToSync []string
			if explicitIDE != "" {
				idesToSync = []string{explicitIDE}
			} else if lfErr == nil && lf != nil && len(lf.IDEs) > 0 {
				idesToSync = hub.FilterSupportedIDEs(lf.IDEs)
			}

			runSyncPhase1(ctx, wd, idesToSync, p)
			stampSync(wd)

			noBg, _ := cmd.Flags().GetBool("no-background")
			if noBg {
				runSyncHeavyTasks(ctx, wd, p)
			} else {
				spawnBackgroundSync(wd, "")
			}

			return nil
		},
	}
	cmd.Flags().String("ide", "", "Target IDE (antigravity, cursor, claude, gemini, kiro, codex, opencode)")
	registerIDEFlagCompletion(cmd)
	cmd.Flags().Bool("no-background", false, "Run all tasks synchronously in the same terminal")
	cmd.Flags().Bool("heavy", false, "Run only heavy tasks (embeddings, memory GC) with terminal output")
	cmd.Flags().Duration("debounce", 0,
		"Skip the sync when one finished less than this ago (used by the git hooks)")
	return cmd
}

// syncStateFile locates one of the sync's coordination files inside the project's
// runtime directory — the same tree the daemon already writes its log to, so
// nothing new needs ignoring or cleaning up.
func syncStateFile(wd, name string) string {
	return brand.ProjectRuntimePath(wd, name)
}

// acquireSyncLock takes the named lock and reports whether the caller should go ahead.
//
// Three outcomes, and only one of them stops the work: the lock is free (proceed with
// it held), someone else holds it (skip — they are doing this already), or the lock
// itself could not be created. The last case proceeds WITHOUT a lock: an unwritable
// brand directory is a reason to log and degrade to the old unsynchronized behaviour,
// never a reason to silently stop syncing the project.
//
// A nil *lockfile.Lock is safe to Release, so callers can defer unconditionally.
func acquireSyncLock(wd, name string) (*lockfile.Lock, bool) {
	lock, err := lockfile.TryAcquire(syncStateFile(wd, name))
	switch {
	case err == nil:
		return lock, true
	case errors.Is(err, lockfile.ErrLocked):
		return nil, false
	default:
		syncLogError("lock", "%s: %v", name, err)
		return nil, true
	}
}

// syncedWithin reports whether a full sync finished less than window ago. A missing or
// unreadable stamp reads as "no idea", which runs the sync — the debounce may only
// ever skip work it can prove is redundant.
func syncedWithin(wd string, window time.Duration) bool {
	if window <= 0 {
		return false
	}
	data, err := os.ReadFile(syncStateFile(wd, "sync.stamp"))
	if err != nil {
		return false
	}
	last, err := time.Parse(time.RFC3339, strings.TrimSpace(string(data)))
	if err != nil {
		return false
	}
	return time.Since(last) < window
}

// stampSync records that Phase 1 completed, which is what syncedWithin reads.
//
// It is written after Phase 1 and not after Phase 2 on purpose: Phase 1 is the part a
// git hook exists to trigger, and Phase 2 is fire-and-forget by design, so waiting for
// it would leave the window open for the whole length of an embedding run.
func stampSync(wd string) {
	path := syncStateFile(wd, "sync.stamp")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return
	}
	_ = os.WriteFile(path, []byte(time.Now().UTC().Format(time.RFC3339)+"\n"), 0o644)
}

func runSyncHeavyTasks(ctx context.Context, wd string, p *output.Printer) {
	projectCfg := loadProjectConfigFromDir(wd)

	if !config.IsModuleDisabled("embedding", nil, projectCfg) {
		var task *output.Task
		if p != nil {
			task = p.StartTask("Generating embeddings and finalizing vector index...")
		}
		cfg := ast.DefaultEmbeddingConfig()
		cfg.RepoRoot = wd
		cfg.ProjectDir = wd
		ladybugCfg := ast.DefaultLadybugConfig()
		cacheDir := ladybugCfg.StoreDir
		parseCache, cacheErr := ast.NewShardCache(cacheDir)
		if cacheErr == nil {
			parseCache.SetRoot(cfg.RepoRoot)
			cfg.ParseCache = parseCache
		}
		searchIdx, idxErr := ast.OpenSearchIndex(ctx, cacheDir)
		if idxErr == nil {
			cfg.Index = searchIdx
			defer func() { _ = searchIdx.Close() }()
		}
		if cfg.ParseCache != nil {
			if embCache, embErr := ast.NewShardEmbCache(cacheDir, cfg.ParseCache); embErr == nil {
				cfg.EmbCache = embCache
				defer func() { _ = embCache.Close() }()
			}
		}

		var embClient ai.EmbeddingClient
		if cfg.Index == nil || cfg.ParseCache == nil || cfg.EmbCache == nil {
			syncLogError("embedding", "search index or embedding caches are unavailable")
			if task != nil {
				task.Fail("Vector index: search index or caches unavailable")
			}
		} else {
			pending := ast.NewEmbedder(nil, cfg).CountPending(ctx)
			if pending > 0 {
				var err error
				embClient, err = ai.NewEmbeddingClientFromConfig()
				if err != nil {
					syncLogError("embedding", "client init: %v", err)
					if task != nil {
						task.Fail("Embeddings: %v", err)
					}
				}
			}
			if pending == 0 || embClient != nil {
				embedder := ast.NewEmbedder(embClient, cfg)
				n, err := embedder.RunCycle(ctx)
				if err != nil {
					syncLogError("embedding", "cycle: %v", err)
					if task != nil {
						task.Fail("Vector index: %v", err)
					}
				} else if task != nil {
					task.Done("Vector index finalized (%d embeddings generated)", n)
				}
			}
		}
	}

	if st, err := hub.NewS3Store(ctx, nil, projectCfg); err == nil {
		var task *output.Task
		if p != nil {
			task = p.StartTask("Syncing background events...")
		}
		st.SyncEvents(ctx)
		if task != nil {
			task.Done("Events synced")
		}

		var reconTask *output.Task
		if p != nil {
			reconTask = p.StartTask("Reconciling hub artifacts...")
		}
		reg, regErr := hub.NewRegistryManager(ctx)
		if regErr == nil {
			if err := hub.ReconcileManagedArtifactsFromDir(reg, wd); err != nil {
				syncLogError("hub-reconcile", "reconcile: %v", err)
				if reconTask != nil {
					reconTask.Fail("Reconcile: %v", err)
				}
			} else if reconTask != nil {
				reconTask.Done("Hub artifacts reconciled")
			}
		} else {
			syncLogError("hub-reconcile", "registry init: %v", regErr)
			if reconTask != nil {
				reconTask.Fail("Registry: %v", regErr)
			}
		}

		// Ensure language artifacts are materialized in global dirs.
		lockPath := filepath.Join(wd, brand.LockFileName())
		if err := hub.EnsureGlobalLanguageArtifacts(lockPath); err != nil {
			syncLogError("hub-lang-global", "ensure global language: %v", err)
		}
	}

	if !config.IsModuleDisabled("memory", nil, projectCfg) {
		var task *output.Task
		if p != nil {
			task = p.StartTask("Running memory maintenance...")
		}
		runMemoryMaintenance(ctx, wd)
		if task != nil {
			task.Done("Memory maintenance complete")
		}
	}
}

// runSyncPhase1 runs the synchronous sync tasks (AST reindex, knowledge/memory
// wiki reindex, hub/memory repo sync, IDE rules & adapter, git hooks).
// It is used by both the "init" and "sync" commands.
func runSyncPhase1(ctx context.Context, wd string, idesToSync []string, p *output.Printer) {
	projectCfg := loadProjectConfigFromDir(wd)

	if !config.IsModuleDisabled("ast", nil, projectCfg) {
		task := p.StartTask("Reindexing AST graph...")
		absPath, _ := filepath.Abs(wd)
		db, err := newASTBackend()
		if err != nil {
			task.Fail("AST backend: %v", err)
		} else {
			ladybugCfg := ast.DefaultLadybugConfig()
			pipeOpts := ast.PipelineOptions{
				Workers:          ast.SafeWorkers(0),
				IndexSource:      config.ResolveIndexSource(nil, projectCfg),
				CacheDir:         ladybugCfg.StoreDir,
				GrammarOverrides: config.ResolveGrammarOverrides(nil, projectCfg),
			}
			result, err := ast.RunPipeline(ctx, db, absPath, pipeOpts)
			if err != nil {
				task.Fail("AST index: %v", err)
			} else if result.ErrorCount > 0 && result.ParsedFiles == 0 {
				task.Fail("AST: %d files discovered, %d parse errors (grammars may be missing)", result.TotalFiles, result.ErrorCount)
			} else if result.ErrorCount > 0 {
				task.Done("AST: %d files indexed, %d errors (%.1fs)", result.ParsedFiles, result.ErrorCount, result.TotalTime.Seconds())
			} else if result.ParsedFiles == 0 {
				task.Done("AST: %d files up to date", result.TotalFiles)
			} else {
				task.Done("AST: %d files indexed (%.1fs)", result.ParsedFiles, result.TotalTime.Seconds())
			}
			_ = db.Close()
		}
	}

	if !config.IsModuleDisabled("knowledge", nil, projectCfg) {
		task := p.StartTask("Reindexing knowledge wiki...")
		scope := knowledge.ScopeFor(wd, nil, projectCfg)
		_, docsErr := os.Stat(filepath.Join(wd, scope.Subdir))
		// A project with no docs tree yet still has a README to index, so the
		// missing directory is only the end of the story when there is nothing else.
		if docsErr == nil || len(scope.ExtraFiles) > 0 {
			wikiDir := knowledge.WikiDir()
			cfg := knowledge.IndexConfig{UseLouvain: false, ProjectCfg: projectCfg, Scope: scope}
			if _, err := knowledge.RunIndexPipeline(ctx, wd, wikiDir, cfg); err != nil {
				task.Fail("Knowledge index: %v", err)
			} else if docsErr == nil {
				task.Done("Knowledge wiki reindexed")
			} else {
				task.Done("Knowledge wiki reindexed (no %s/ yet — %s only)",
					scope.Subdir, strings.Join(scope.ExtraFiles, ", "))
			}
		} else {
			task.Done("No %s/ directory and no README — skipping", scope.Subdir)
		}
	}

	task := p.StartTask("Syncing memory repository...")
	memStore, err := memory.NewMemoryStore()
	if err != nil {
		task.Fail("Memory store: %v", err)
	} else {
		type syncResult struct{ err error }
		projResult := make(chan syncResult, 1)
		userResult := make(chan syncResult, 1)

		var wg sync.WaitGroup
		wg.Add(2)
		go func() {
			defer wg.Done()
			if projSvc, _, svcErr := newMemorySvc(false); svcErr == nil {
				projResult <- syncResult{err: projSvc.SyncWiki()}
				_ = projSvc.Close()
			} else {
				projResult <- syncResult{}
			}
		}()
		go func() {
			defer wg.Done()
			if userSvc, _, svcErr := newMemorySvc(true); svcErr == nil {
				userResult <- syncResult{err: userSvc.SyncWiki()}
				_ = userSvc.Close()
			} else {
				userResult <- syncResult{}
			}
		}()
		wg.Wait()

		syncOK := true
		if r := <-projResult; r.err != nil {
			p.StepWarn("Memory project sync: %v", r.err)
			syncOK = false
		}
		if r := <-userResult; r.err != nil {
			p.StepWarn("Memory user sync: %v", r.err)
			syncOK = false
		}
		_ = memStore
		if syncOK {
			task.Done("Memory repository synced")
		}
	}

	if !config.IsModuleDisabled("memory", nil, projectCfg) {
		task = p.StartTask("Reindexing memory wikis...")
		memory.RunProjectCycle(ctx)
		memory.RunUserCycle(ctx)
		task.Done("Memory wikis reindexed")
	}

	task = p.StartTask("Syncing hub registry...")
	st, err := hub.NewS3Store(ctx, nil, loadProjectConfig())
	if err != nil {
		task.Fail("Hub not configured: %v", err)
	} else if err := st.SyncRegistry(ctx); err != nil {
		task.Fail("Hub sync: %v", err)
	} else {
		task.Done("Hub registry synced")
	}

	task = p.StartTask("Updating IDE rules...")
	for _, targetIDE := range idesToSync {
		installAllRules(p, wd, targetIDE)
	}
	task.Done("IDE rules updated")

	task = p.StartTask("Syncing IDE adapter...")
	lf, lfErr := hub.LoadLockfile(filepath.Join(wd, brand.LockFileName()))
	if lfErr == nil && lf != nil {
		var syncErrs []string
		for _, targetIDE := range idesToSync {
			if syncErr := hub.SyncIDEAdapter(targetIDE, wd, lf); syncErr != nil {
				syncErrs = append(syncErrs, fmt.Sprintf("%s: %v", targetIDE, syncErr))
			}
		}
		if len(syncErrs) > 0 {
			task.Fail("IDE adapters sync failed: %s", strings.Join(syncErrs, "; "))
		} else {
			task.Done("IDE adapter synced")
		}
	} else {
		task.Done("No lockfile — skipping IDE adapter sync")
	}

	task = p.StartTask("Syncing git hooks...")
	hm := git.NewHookManager("")
	if config.IsModuleDisabled("hooks", nil, projectCfg) {
		if err := hm.Remove(); err != nil {
			task.Fail("Git hooks removal: %v", err)
		} else {
			task.Done("Git hooks removed (disabled by config)")
		}
	} else {
		if err := hm.Install(false); err != nil {
			task.Fail("Git hooks: %v", err)
		} else {
			task.Done("Git hooks synced")
		}
	}

	p.Success("Sync complete")
}

func spawnBackgroundSync(wd, ide string) {
	// Prefer the launcher — after an upgrade the old graphit-core may be gone.
	exe := os.Getenv(brand.EnvVar("LAUNCHER_PATH"))
	if exe == "" {
		var err error
		exe, err = os.Executable()
		if err != nil {
			syncLogError("spawn", "executable path: %v", err)
			return
		}
		exe, _ = filepath.EvalSymlinks(exe)
	}

	args := []string{"sync", "--heavy"}
	if ide != "" {
		args = append(args, "--ide", ide)
	}

	cmd := exec.Command(exe, args...)
	cmd.Dir = wd
	cmd.Stdin = nil
	cmd.Stdout = nil
	cmd.Stderr = nil

	if err := cmd.Start(); err != nil {
		syncLogError("spawn", "start: %v", err)
		return
	}

	go func() { _ = cmd.Wait() }() // reap child to avoid zombie
}

func syncLogError(module, format string, args ...any) {
	logDir := brand.GlobalDir()
	if logDir == "" {
		return
	}
	logPath := filepath.Join(logDir, "sync.log")
	f, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return
	}
	defer func() { _ = f.Close() }()
	msg := fmt.Sprintf(format, args...)
	_, _ = fmt.Fprintf(f, "%s [sync:%s] %s\n", time.Now().UTC().Format(time.RFC3339), module, msg)
}

// runMemoryMaintenance regenerates the memory wiki for each scope.
//
// It used to garbage-collect first: RunGC(90) followed by os.Remove straight into
// the worktree. That is gone deliberately. Deleting memories is not maintenance —
// it was destructive work with no report, no confirmation and no undo, triggered
// by a command a developer runs to *index* things, bypassing MemoryService so
// neither git nor the wiki knew a memory had ever existed. Age is also the weakest
// possible evidence: the memories that sit unread for months are exactly the
// conventions and corrections that later stop a repeated mistake.
//
// Sanitising the store is consolidation's job — the dream module on idle, or
// `memory consolidate` on demand. There the model judges what actually duplicates or
// contradicts what, every removal carries the content into a survivor first, and the
// result is a report the developer can read. Deletion by age is gone entirely: GC was
// removed rather than relocated.
func runMemoryMaintenance(ctx context.Context, projectDir string) {
	_ = projectDir
	for _, scope := range []string{"project", "user"} {
		wikiDir := memory.WikiDir(scope)
		if wikiDir == "" {
			continue
		}
		memory.RunCycle(ctx, scope, memory.TableURIForScope(scope), wikiDir)
	}
}

func newUninstallCmd() *cobra.Command {
	var removeAll bool

	cmd := &cobra.Command{
		Use:   "uninstall",
		Short: "Remove " + brand.DisplayName + " global resources (caches, repositories)",
		Long: `Uninstall ` + brand.DisplayName + ` global resources.

This command:
  • Cleans hub repository caches
  • Cleans memory repository worktrees and caches
  • Removes other transient caches from ~/` + brand.DotDir() + `/

By default, your global configuration (~/` + brand.DotDir() + `/config.json) and
custom rules (~/` + brand.DotDir() + `/rules/) are preserved.

Use --all to remove the entire ~/` + brand.DotDir() + `/ directory, including
configuration and custom rules. This is a destructive operation.`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			p.Header("Uninstalling %s", brand.DisplayName)

			if st, err := hub.NewS3Store(cmd.Context(), nil, nil); err == nil {
				tracker := hub.NewEventTracker(st)
				tracker.TrackEvent("global.uninstall", "", nil, nil)
			}

			pid := daemon.NewPIDFile()
			if alive := pid.IsAlive(); alive != nil {
				task := p.StartTask("Stopping daemon (pid %d)...", alive.PID)
				_ = pid.Signal(os.Interrupt)

				for i := 0; i < 10; i++ {
					time.Sleep(500 * time.Millisecond)
					if pid.IsAlive() == nil {
						break
					}
				}
				if pid.IsAlive() != nil {
					_ = pid.Signal(os.Kill)
					pid.Remove()
				}
				task.Done("Daemon stopped")
			}

			if daemon.IsSchedulerInstalled() {
				task := p.StartTask("Removing OS scheduler...")
				if err := daemon.RemoveScheduler(); err != nil {
					task.Fail("Scheduler removal: %v", err)
				} else {
					task.Done("OS scheduler removed")
				}
			}

			globalDir := brand.GlobalDir()
			if globalDir == "" {
				return fmt.Errorf("cannot determine home directory")
			}

			if removeAll {

				task := p.StartTask("Removing %s/...", brand.DotDir())
				if err := os.RemoveAll(globalDir); err != nil {
					task.Fail("Could not remove %s: %v", globalDir, err)
				} else {
					task.Done("Removed %s", globalDir)
				}
			} else {

				cacheDirs := []string{
					"hub",
					"memory",
					"knowledge",
					"ast",
				}

				for _, dir := range cacheDirs {
					dirPath := filepath.Join(globalDir, dir)
					if _, err := os.Stat(dirPath); os.IsNotExist(err) {
						continue
					}
					task := p.StartTask("Cleaning %s/...", dir)
					if err := os.RemoveAll(dirPath); err != nil {
						task.Fail("Could not remove %s: %v", dirPath, err)
					} else {
						task.Done("Removed ~/%s/%s", brand.DotDir(), dir)
					}
				}
			}

			p.Blank()
			if removeAll {
				p.Success("Uninstall complete — all %s data removed", brand.DisplayName)
			} else {
				p.Success("Uninstall complete — caches cleaned, config preserved")
				p.Step("Your configuration is still at ~/%s/config.json", brand.DotDir())
				p.Step("Custom rules are still at ~/%s/rules/", brand.DotDir())
				p.Step("Use --all to remove everything")
			}
			return nil
		},
	}
	cmd.Flags().BoolVar(&removeAll, "all", false, "Remove the entire ~/"+brand.DotDir()+"/ directory (including config and rules)")
	return cmd
}
