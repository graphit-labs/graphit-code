package commands

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newSetupCmd() *cobra.Command {

	cmd := &cobra.Command{
		Use:   "setup",
		Short: "Configure " + brand.DisplayName + " (hub repository, memory repository, default IDE, default CLI)",
		Long: `Interactive setup for ` + brand.DisplayName + `.

This command configures the essential settings:
  • Hub Git repository URL (where artifacts and releases are stored)
  • Memory Git repository URL (optional remote for persistent memories)
  • Default IDE (used when --ide is not explicitly provided)
  • Default CLI (used for AI fallback when API keys are missing)

Settings are stored in ~/` + brand.DotDir() + `/config.json (global config).
The memory repository is always initialised at ~/` + brand.DotDir() + `/memory/
(a standalone Git repo, completely independent of the hub repository).`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			reader := bufio.NewReader(os.Stdin)

			if _, err := exec.LookPath("git"); err != nil {
				p.Error("git is required but was not found in PATH")
				p.Detail("Install git", "https://git-scm.com/downloads")
				return fmt.Errorf("git CLI not found in PATH: %w", err)
			}

			p.Header("Welcome to %s setup", brand.DisplayName)

			currentRepo := config.HubRepoURL()
			hubDefault := currentRepo
			if hubDefault == "" {
				hubDefault = brand.DefaultHubRepoURL
			}
			if currentRepo != "" {
				p.Detail("Current hub repository", currentRepo)
			}
			if hubDefault != "" {
				fmt.Printf("  Enter hub repository URL [%s]: ", hubDefault)
			} else {
				fmt.Print("  Enter hub repository URL [leave blank for local-only mode]: ")
			}
			repoInput, _ := reader.ReadString('\n')
			repoInput = strings.TrimSpace(repoInput)

			if repoInput == "" {
				if hubDefault != "" {
					repoInput = hubDefault
					p.Step("Using default: %s", repoInput)
				} else if currentRepo != "" {

					fmt.Printf("  Keep current hub repository %q? [Y/n]: ", currentRepo)
					keepInput, _ := reader.ReadString('\n')
					keepInput = strings.TrimSpace(strings.ToLower(keepInput))
					if keepInput == "" || keepInput == "y" || keepInput == "yes" {
						repoInput = currentRepo
						p.Step("Keeping current: %s", repoInput)
					} else {
						_ = config.UnsetGlobalConfigValue("hub.repo")
						p.StepOK("Hub repository: local-only (no remote)")
						repoInput = ""
					}
				}
			}
			if repoInput != "" {
				if err := config.SetGlobalConfigValue("hub.repo", repoInput); err != nil {
					return fmt.Errorf("saving hub.repo: %w", err)
				}
				p.StepOK("Hub repository: %s", repoInput)
			} else if currentRepo == "" {
				p.StepOK("Hub repository: local-only (no remote configured)")
			}

			currentMemoryRepo := config.MemoryRepoURL()
			memDefault := currentMemoryRepo
			if memDefault == "" {
				memDefault = brand.DefaultMemoryRepoURL
			}
			if currentMemoryRepo != "" {
				p.Detail("Current memory repository", currentMemoryRepo)
			}
			if memDefault != "" {
				fmt.Printf("  Enter memory repository URL [%s]: ", memDefault)
			} else {
				fmt.Print("  Enter memory repository URL [leave blank for offline mode]: ")
			}
			memRepoInput, _ := reader.ReadString('\n')
			memRepoInput = strings.TrimSpace(memRepoInput)

			if memRepoInput == "" {
				if memDefault != "" {
					memRepoInput = memDefault
					p.Step("Using default: %s", memRepoInput)
				} else if currentMemoryRepo != "" {

					fmt.Printf("  Keep current memory repository %q? [Y/n]: ", currentMemoryRepo)
					keepInput, _ := reader.ReadString('\n')
					keepInput = strings.TrimSpace(strings.ToLower(keepInput))
					if keepInput == "" || keepInput == "y" || keepInput == "yes" {
						memRepoInput = currentMemoryRepo
						p.Step("Keeping current: %s", memRepoInput)
					} else {
						_ = config.UnsetGlobalConfigValue("memory.repo")
						p.StepOK("Memory repository: offline (no remote)")
						memRepoInput = ""
					}
				}
			}
			if memRepoInput != "" {
				if err := config.SetGlobalConfigValue("memory.repo", memRepoInput); err != nil {
					return fmt.Errorf("saving memory.repo: %w", err)
				}
				p.StepOK("Memory repository: %s", memRepoInput)
			} else if currentMemoryRepo == "" {
				p.StepOK("Memory repository: offline (no remote configured)")
			}

			currentIDE := config.DefaultIDE()
			fmt.Printf("  Enter default IDE [%s]: ", currentIDE)
			ideInput, _ := reader.ReadString('\n')
			ideInput = strings.TrimSpace(ideInput)

			if ideInput == "" {
				ideInput = currentIDE
			}

			if err := config.SetGlobalConfigValue("ide", ideInput); err != nil {
				return fmt.Errorf("saving ide: %w", err)
			}
			p.StepOK("Default IDE: %s", ideInput)

			currentCLI := config.DefaultCLI()
			fmt.Printf("  Enter default CLI [%s]: ", currentCLI)
			cliInput, _ := reader.ReadString('\n')
			cliInput = strings.TrimSpace(cliInput)

			if cliInput == "" {
				cliInput = currentCLI
			}

			if err := config.SetGlobalConfigValue("cli", cliInput); err != nil {
				return fmt.Errorf("saving cli: %w", err)
			}
			p.StepOK("Default CLI: %s", cliInput)

			p.Blank()
			task := p.StartTask("Initialising hub repository...")

			gs, err := hub.NewGitStore(nil, nil)
			if err != nil {
				task.Fail("Hub init failed: %v", err)
				return fmt.Errorf("initializing hub: %w", err)
			}
			if err := gs.EnsureCloned(); err != nil {

				task.Fail("Hub sync failed (will retry on next command): %v", err)
			} else {
				task.Done("Hub repository ready at %s", gs.Dir())
			}

			memTask := p.StartTask("Initialising memory repository...")

			memStore, err := memory.NewMemoryGitStore()
			if err != nil {
				memTask.Fail("Memory store failed: %v", err)
				return fmt.Errorf("resolving memory store path: %w", err)
			}
			if err := memStore.EnsureInitialised(); err != nil {

				memTask.Fail("Memory repository init: %v", err)
			} else {
				memTask.Done("Memory repository ready at %s", memStore.Dir())
			}

			p.Blank()
			p.Success("Setup complete! Run '%s init' to initialize a project.", brand.BinName())

			if !config.IsModuleDisabled("daemon", nil, nil) {
				_, _ = daemon.EnsureRunning()
			}

			tracker := hub.NewEventTracker(gs)
			tracker.TrackEvent("global.setup", "", nil, map[string]string{
				"ide": ideInput,
				"cli": cliInput,
			})

			return nil
		},
	}
	return cmd
}
