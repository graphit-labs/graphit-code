package commands

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"syscall"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newDaemonCmd() *cobra.Command {
	var (
		noEmbedding bool
		noDream     bool
		logPath     string
	)

	cmd := &cobra.Command{
		Use:   "daemon",
		Short: "Global background process — keeps embedding model warm and runs autonomous tasks",
		Long: brand.DisplayName + ` Daemon — a single global process that discovers all projects
and keeps expensive resources (ONNX embedding model) loaded in memory, shared
across all projects.

The daemon is auto-started by any CLI command and runs until explicitly stopped
or until a binary update is detected.

The daemon:
  • Discovers all active projects from the global lock
  • Keeps the embedding model (CodeRankEmbed-137M) loaded — shared across projects
  • Periodically scans for new entities and computes embeddings in background
  • Runs the autonomous dream module during idle periods (per project)
  • Automatically recovers crashed modules with exponential backoff

Lifecycle:
  ` + brand.BinName() + ` daemon                          Start in foreground (Ctrl+C to stop)
  ` + brand.BinName() + ` daemon stop                      Stop the running daemon
  ` + brand.BinName() + ` daemon status                    Show daemon health
  ` + brand.BinName() + ` daemon restart                   Stop + start

Auto-start (OS scheduler):
  ` + brand.BinName() + ` daemon scheduler install         Register daemon auto-start with OS
  ` + brand.BinName() + ` daemon scheduler remove          Unregister daemon auto-start
  ` + brand.BinName() + ` daemon scheduler status          Show scheduler status

PID file: ~/` + brand.DotDir() + `/daemon/daemon.pid (global, one daemon per machine)`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDaemonStart(noEmbedding, noDream, logPath)
		},
	}

	cmd.Flags().BoolVar(&noEmbedding, "no-embedding", false, "Disable background embedding module")
	cmd.Flags().BoolVar(&noDream, "no-dream", false, "Disable autonomous dream module")
	cmd.Flags().StringVar(&logPath, "log", "", "Log file path (default: ~/"+brand.DotDir()+"/daemon/daemon.log)")

	cmd.AddCommand(
		newDaemonStopCmd(),
		newDaemonStatusCmd(),
		newDaemonRestartCmd(),
		newDaemonSchedulerCmd(),
	)

	return cmd
}

func runDaemonStart(noEmbedding, noDream bool, logPath string) error {

	maxProcs := runtime.NumCPU() / 2
	if maxProcs < 2 {
		maxProcs = 2
	}
	if maxProcs > 8 {
		maxProcs = 8
	}
	runtime.GOMAXPROCS(maxProcs)

	cfg := daemon.DefaultConfig()
	cfg.DisableEmbedding = noEmbedding
	cfg.DisableDream = noDream

	if logPath != "" {
		cfg.LogPath = logPath
	}

	var sharedEmbedClient ai.EmbeddingClient
	if !cfg.DisableEmbedding {
		sharedEmbedClient = ai.NewLazyEmbeddingClient()

		go func() {
			if mgr, err := ai.NewModelManager(); err == nil {
				_, _, _ = mgr.EnsureModel(context.Background())
			}
		}()
	}

	builder := func(projectDir string) ([]daemon.WatchModule, []func() error, error) {
		var modules []daemon.WatchModule
		var closerFns []func() error

		lp := filepath.Join(projectDir, brand.LockFileName())
		lf, _ := hub.LoadLockfile(lp)
		var projectCfg config.ConfigMap
		if lf != nil {
			projectCfg = lf.Config
		}

		disableEmbedding := cfg.DisableEmbedding || sharedEmbedClient == nil || config.IsModuleDisabled("embedding", nil, projectCfg)
		disableDream := cfg.DisableDream || config.IsModuleDisabled("dream", nil, projectCfg)

		cacheDir := filepath.Join(projectDir, brand.DotDir(), "ast", "project")

		if !disableEmbedding {
			modules = append(modules, daemon.NewEmbeddingModule(projectDir, 2*time.Minute, cacheDir))
		}
		if !disableDream {
			var lockfileIDEs []string
			if lf != nil {
				lockfileIDEs = lf.IDEs
			}
			ide := config.ResolveProjectIDE("", nil, projectCfg, lockfileIDEs)
			modules = append(modules, daemon.NewDreamModule(projectDir, ide))
		}

		return modules, closerFns, nil
	}

	signal.Reset(syscall.SIGTERM, syscall.SIGINT)
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	if sharedEmbedClient != nil {
		go func() {
			server := daemon.NewEmbedServer(sharedEmbedClient)
			_ = server.Start(ctx)
		}()
	}

	d := daemon.New(cfg, builder)

	discoverFn := func() ([]daemon.ProjectInfo, error) {
		mgr, err := hub.NewGlobalLockManager()
		if err != nil {
			return nil, err
		}
		active, err := mgr.ListActiveProjects()
		if err != nil {
			return nil, err
		}
		result := make([]daemon.ProjectInfo, 0, len(active))
		for _, p := range active {
			result = append(result, daemon.ProjectInfo{ID: p.ID, Dir: p.Dir})
		}
		return result, nil
	}

	return d.Start(ctx, discoverFn)
}

func newDaemonStopCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "stop",
		Short: "Stop the running global daemon",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			pid := daemon.NewPIDFile()

			alive := pid.IsAlive()
			if alive == nil {
				p.Info("No daemon running.")
				return nil
			}

			p.Running("Stopping daemon (pid %d)…", alive.PID)
			if err := pid.Signal(syscall.SIGTERM); err != nil {
				return fmt.Errorf("sending SIGTERM: %w", err)
			}

			for i := 0; i < 20; i++ {
				time.Sleep(500 * time.Millisecond)
				if pid.IsAlive() == nil {
					p.Success("Daemon stopped")
					return nil
				}
			}

			p.Warn("Daemon did not stop within 10s — sending SIGKILL")
			if err := pid.Signal(syscall.SIGKILL); err != nil {
				return fmt.Errorf("sending SIGKILL: %w", err)
			}
			pid.Remove()
			p.Success("Daemon killed")
			return nil
		},
	}
}

func newDaemonStatusCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show global daemon status",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			pid := daemon.NewPIDFile()

			alive := pid.IsAlive()
			if alive == nil {
				p.Info("No daemon running.")
				p.Step("Start one with: %s daemon", brand.BinName())
				return nil
			}

			uptime := time.Since(alive.StartedAt).Truncate(time.Second)
			p.Header("Daemon Status")
			p.KeyValue("PID", fmt.Sprintf("%d", alive.PID))
			p.KeyValue("Started", alive.StartedAt.Format(time.RFC3339))
			p.KeyValue("Uptime", uptime.String())
			p.KeyValue("PID File", pid.Path())
			p.KeyValue("Scope", "global (all projects)")
			p.KeyValue("Scheduler", daemon.SchedulerStatus())

			logPath := filepath.Join(daemon.GlobalDaemonDir(), "daemon.log")
			if data, err := os.ReadFile(logPath); err == nil {
				lines := splitLastN(string(data), 10)
				if len(lines) > 0 {
					p.Header("Recent Log")
					for _, line := range lines {
						p.Step("%s", line)
					}
				}
			}

			return nil
		},
	}
}

func splitLastN(s string, n int) []string {
	lines := make([]string, 0)
	for _, line := range splitLines(s) {
		if line != "" {
			lines = append(lines, line)
		}
	}
	if len(lines) > n {
		lines = lines[len(lines)-n:]
	}
	return lines
}

func splitLines(s string) []string {
	var lines []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			lines = append(lines, s[start:i])
			start = i + 1
		}
	}
	if start < len(s) {
		lines = append(lines, s[start:])
	}
	return lines
}

func newDaemonRestartCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "restart",
		Short: "Stop and restart the global daemon",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			pid := daemon.NewPIDFile()

			if alive := pid.IsAlive(); alive != nil {
				p.Running("Stopping daemon (pid %d)…", alive.PID)
				_ = pid.Signal(syscall.SIGTERM)
				for i := 0; i < 20; i++ {
					time.Sleep(500 * time.Millisecond)
					if pid.IsAlive() == nil {
						break
					}
				}
				if pid.IsAlive() != nil {
					_ = pid.Signal(syscall.SIGKILL)
					pid.Remove()
				}
				p.StepOK("Previous daemon stopped")
			}

			p.Running("Starting global daemon…")
			return runDaemonStart(false, false, "")
		},
	}
}

func newDaemonSchedulerCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "scheduler",
		Short: "Manage OS-level daemon auto-start (cron/launchd/schtasks)",
		Long: `Manage the OS-level scheduler that auto-starts the daemon.

The scheduler uses user-scoped, privilege-free mechanisms:
  • Linux:   User crontab entry (runs every 1 minute)
  • macOS:   LaunchAgent plist (RunAtLoad + periodic restart)
  • Windows: Task Scheduler entry (runs every 1 minute)

The scheduler is completely optional — the daemon is also auto-started
by any CLI command. The scheduler ensures the daemon stays running even
when no CLI commands are being executed (e.g. during long coding sessions).`,
	}

	cmd.AddCommand(
		newDaemonSchedulerInstallCmd(),
		newDaemonSchedulerRemoveCmd(),
		newDaemonSchedulerStatusCmd(),
	)

	return cmd
}

func newDaemonSchedulerInstallCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "install",
		Short: "Register the daemon with the OS scheduler for auto-start",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			p.Running("Installing OS scheduler...")
			if err := daemon.InstallScheduler(); err != nil {
				p.Error("Failed: %v", err)
				return err
			}

			p.Success("Scheduler installed")
			p.Step("Status: %s", daemon.SchedulerStatus())
			return nil
		},
	}
}

func newDaemonSchedulerRemoveCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "remove",
		Short: "Unregister the daemon from the OS scheduler",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")

			p.Running("Removing OS scheduler...")
			if err := daemon.RemoveScheduler(); err != nil {
				p.Error("Failed: %v", err)
				return err
			}

			p.Success("Scheduler removed")
			return nil
		},
	}
}

func newDaemonSchedulerStatusCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show the current OS scheduler status",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			p.KeyValue("Scheduler", daemon.SchedulerStatus())
			return nil
		},
	}
}
