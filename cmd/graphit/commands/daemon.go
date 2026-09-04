package commands

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/mcpstdio"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
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
		Short: "Global background process — maintains indexes and shared services",
		Long: brand.DisplayName + ` Daemon — a single global process that discovers all projects
and keeps expensive resources (such as the configured embedding backend) shared
across all projects.

The daemon is auto-started by ordinary CLI commands unless modules.daemon=false,
and runs until explicitly stopped or until a binary/grammar update is detected.

The daemon:
  • Discovers all registered projects from the global lock
  • Parks projects that have gone quiet (daemon.activity_window, default 30m) —
    their fs watch, embedding loop and dream runner all stop — and resumes
    watching the moment a parked project changes again
  • Shares the configured local or remote embedding backend across projects
  • Periodically scans for new entities and computes embeddings in background
  • Runs the autonomous dream module during idle periods (per project)
  • Automatically recovers crashed modules with exponential backoff

Filesystem watch (enabled by default):
  ` + brand.BinName() + ` config modules.sync false             Disable for the current project
  ` + brand.BinName() + ` config --global modules.sync false    Disable for every project
  GRAPHIT_MODULES_SYNC=false ` + brand.BinName() + ` daemon      Disable through the environment

Restart the daemon after changing project or global configuration. Disabling
modules.sync removes the per-project watcher and incremental AST/Knowledge
updates; explicit ` + brand.BinName() + ` sync and direct index commands remain available.

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
	closeMCP, err := runDaemonCore(noEmbedding, noDream, logPath)
	if !errors.Is(err, daemon.ErrReplace) {
		return err
	}
	closeMCP()

	exe := daemonctl.ResolveExe()
	if exe == "" {
		exe, _ = os.Executable()
	}
	argv := []string{"daemon"}
	if noEmbedding {
		argv = append(argv, "--no-embedding")
	}
	if noDream {
		argv = append(argv, "--no-dream")
	}
	if logPath != "" {
		argv = append(argv, "--log", logPath)
	}
	if spawnErr := spawnDetachedDaemon(exe, argv); spawnErr != nil {
		return fmt.Errorf("spawning new daemon: %w", spawnErr)
	}
	return nil
}

func spawnDetachedDaemon(exe string, argv []string) error {
	cmd := exec.Command(exe, argv...)
	cmd.Stdin = nil
	cmd.Stdout = nil
	closeLog := daemon.AttachLogStderr(cmd)
	defer closeLog()
	sysutil.DetachProcess(cmd)
	return cmd.Start()
}

func runDaemonCore(noEmbedding, noDream bool, logPath string) (closeMCP func(), err error) {
	var (
		mcpOnce     sync.Once
		mcpCloserMu sync.Mutex
		mcpCloserFn func()
	)
	runCloseMCP := func() {
		mcpOnce.Do(func() {
			mcpCloserMu.Lock()
			fn := mcpCloserFn
			mcpCloserMu.Unlock()
			if fn != nil {
				fn()
			}
		})
	}
	closeMCP = runCloseMCP

	runtime.GOMAXPROCS(sysutil.CPUBudget())

	if err := sysutil.LowerPriority(); err != nil {
		output.NewPrinter("daemon").Warn("could not lower process priority: %v", err)
	}

	cfg := daemon.DefaultConfig()
	cfg.DisableEmbedding = noEmbedding
	cfg.DisableDream = noDream
	cfg.ProjectActivityWindow = config.ResolveProjectActivityWindow(nil, nil)

	if logPath != "" {
		cfg.LogPath = logPath
	}

	var sharedEmbedClient ai.EmbeddingClient
	if !cfg.DisableEmbedding {
		sharedEmbedClient = ai.NewLazyEmbeddingClient()

		embeddingProvider := strings.ToLower(strings.TrimSpace(config.ResolveConfig("ai.embedding.provider", nil, nil)))
		if embeddingProvider == "" || embeddingProvider == "local" {
			go func() {
				if mgr, mgErr := ai.NewModelManager(); mgErr == nil {
					_, _, _ = mgr.EnsureModel(context.Background())
				}
			}()
		}
	}

	builder := func(projectDir string) ([]daemon.WatchModule, []func() error, error) {
		return buildDaemonProjectModules(projectDir, cfg, sharedEmbedClient)
	}

	signal.Reset(syscall.SIGTERM, syscall.SIGINT)
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	mcpPortFile := daemonctl.PortFilePath()
	mcpKeyFile := daemonctl.KeyFilePath()
	pidClaimed := make(chan struct{})
	mcpReady := make(chan struct{})
	go func() {
		defer func() {
			select {
			case <-mcpReady:
			default:
				close(mcpReady)
			}
		}()

		apiKey, genErr := mcpproxy.GenerateAPIKey()
		if genErr != nil {
			return
		}

		mcpHandler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
			return mcpstdio.NewServer()
		}, nil)

		authHandler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.Header.Get("Authorization") != "Bearer "+apiKey {
				http.Error(w, "unauthorized", http.StatusUnauthorized)
				return
			}
			mcpHandler.ServeHTTP(w, r)
		})

		mcpHost := config.ResolveMCPHost(nil, nil)
		mcpPort := config.ResolveMCPPort(nil, nil)
		listener, listenErr := net.Listen("tcp", net.JoinHostPort(mcpHost, strconv.Itoa(mcpPort)))
		if listenErr != nil {
			return
		}

		port := listener.Addr().(*net.TCPAddr).Port
		portStr := strconv.Itoa(port)

		select {
		case <-pidClaimed:
		case <-ctx.Done():
			_ = listener.Close()
			return
		}

		_ = os.MkdirAll(filepath.Dir(mcpPortFile), 0o755)
		if err := os.WriteFile(mcpPortFile, []byte(portStr), 0o644); err != nil {
			_ = listener.Close()
			return
		}
		if err := os.WriteFile(mcpKeyFile, []byte(apiKey), 0o600); err != nil {
			_ = listener.Close()
			_ = os.Remove(mcpPortFile)
			return
		}

		func() {
			mcpCloserMu.Lock()
			defer mcpCloserMu.Unlock()
			mcpCloserFn = func() {
				_ = listener.Close()
				if data, rdErr := os.ReadFile(mcpPortFile); rdErr == nil && strings.TrimSpace(string(data)) == portStr {
					_ = os.Remove(mcpPortFile)
					_ = os.Remove(mcpKeyFile)
				}
			}
		}()

		go func() {
			<-ctx.Done()
			runCloseMCP()
		}()

		go func() {
			ticker := time.NewTicker(5 * time.Second)
			defer ticker.Stop()
			for {
				select {
				case <-ctx.Done():
					return
				case <-ticker.C:
					if data, err := os.ReadFile(mcpPortFile); err != nil || strings.TrimSpace(string(data)) != portStr {
						_ = os.WriteFile(mcpPortFile, []byte(portStr), 0o644)
					}
					if data, err := os.ReadFile(mcpKeyFile); err != nil || strings.TrimSpace(string(data)) != apiKey {
						_ = os.WriteFile(mcpKeyFile, []byte(apiKey), 0o600)
					}
				}
			}
		}()

		close(mcpReady)

		mux := http.NewServeMux()
		mux.Handle("/mcp", authHandler)
		httpServer := &http.Server{Handler: mux}
		_ = httpServer.Serve(listener)
	}()

	p := output.NewPrinter("daemon")
	cfg.OnEvent = func(level string, msg string) {
		switch level {
		case "running":
			p.Running("%s", msg)
		case "step":
			p.Step("%s", msg)
		case "step_ok":
			p.StepOK("%s", msg)
		case "step_warn":
			p.StepWarn("%s", msg)
		case "warn":
			p.Warn("%s", msg)
		case "success":
			p.Success("%s", msg)
		case "blank":
			p.Blank()
		}
	}

	d := daemon.New(cfg, builder)

	if sharedEmbedClient != nil {
		d.AddGlobalModule(daemon.NewEmbedServer(sharedEmbedClient))
	}
	if !config.IsModuleDisabled("memory", nil, nil) {
		if userID, userErr := memory.UserScopeID(); userErr == nil && userID != "" {
			d.AddGlobalModule(daemon.NewMemoryMaintenanceModule(
				memory.TableURIFor("user", userID), 15*time.Minute,
			))
		} else if userErr != nil {
			p.Warn("user memory maintenance is disabled: %v", userErr)
		}
	}

	if config.DaemonServesUI(nil, nil) {
		d.AddGlobalModule(newDaemonUIModule(""))
	}

	discoverFn := func() ([]daemon.ProjectInfo, error) {
		mgr, mgrErr := hub.NewGlobalLockManager()
		if mgrErr != nil {
			return nil, mgrErr
		}
		active, actErr := mgr.ListActiveProjects()
		if actErr != nil {
			return nil, actErr
		}
		result := make([]daemon.ProjectInfo, 0, len(active))
		for _, p := range active {
			result = append(result, daemon.ProjectInfo{ID: p.ID, Dir: p.Dir})
		}
		return result, nil
	}

	return closeMCP, d.Start(ctx, discoverFn, func() {
		close(pidClaimed)
		<-mcpReady
	})
}

func buildDaemonProjectModules(projectDir string, cfg daemon.Config, sharedEmbedClient ai.EmbeddingClient) ([]daemon.WatchModule, []func() error, error) {
	lp := filepath.Join(projectDir, brand.LockFileName())
	lf, _ := hub.LoadLockfile(lp)
	var projectCfg config.ConfigMap
	if lf != nil {
		projectCfg = lf.Config
	}

	disableSync := config.IsModuleDisabled("sync", nil, projectCfg)
	disableEmbedding := cfg.DisableEmbedding || sharedEmbedClient == nil || config.IsModuleDisabled("embedding", nil, projectCfg)
	disableDream := cfg.DisableDream || config.IsModuleDisabled("dream", nil, projectCfg)
	disableMemory := config.IsModuleDisabled("memory", nil, projectCfg)
	cacheDir := store.ASTProjectDir(projectDir)

	var modules []daemon.WatchModule
	if !disableSync {
		modules = append(modules, daemon.NewSyncModule(projectDir, cacheDir))
	}
	if !disableEmbedding {
		modules = append(modules, daemon.NewEmbeddingModule(projectDir, 2*time.Minute, cacheDir))
		modules = append(modules, daemon.NewWikiEmbeddingModule(projectDir, daemon.WikiEmbedTargets(projectDir, nil), 2*time.Minute))
	}
	if !disableDream {
		var lockfileIDEs []string
		if lf != nil {
			lockfileIDEs = lf.IDEs
		}
		ide := config.ResolveProjectIDE("", nil, projectCfg, lockfileIDEs)
		modules = append(modules, daemon.NewDreamModule(projectDir, ide))
	}
	if !disableMemory && lf != nil && lf.Project.ID != "" {
		modules = append(modules, daemon.NewMemoryMaintenanceModule(
			memory.TableURIFor("project", lf.Project.ID), 15*time.Minute,
		))
	}

	return modules, nil, nil
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
			_ = os.Remove(daemonctl.PortFilePath())
			_ = os.Remove(daemonctl.KeyFilePath())
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
					_ = os.Remove(daemonctl.PortFilePath())
					_ = os.Remove(daemonctl.KeyFilePath())
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
by ordinary CLI commands. The scheduler ensures the daemon stays running even
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
