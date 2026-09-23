package commands

import (
	"context"
	"crypto/subtle"
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

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/mcpstdio"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
	"github.com/graphit-labs/graphit-code/internal/tray"
	"github.com/spf13/cobra"
)

func newDaemonCmd() *cobra.Command {
	var (
		noEmbedding bool
		noDream     bool
		serveUI     bool
		logPath     string
		managed     bool
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

MCP bearer authentication:
  The daemon always generates a restricted runtime key for local stdio clients.
  Local providers may use their static MCP key. Direct OIDC and Broker-managed providers
  require a verified end-user access token; that same request identity is propagated to
  broker calls. Broker-issued tokens are verified through Broker discovery and userinfo.

Restart the daemon after changing project or global configuration. Disabling
modules.sync removes the per-project watcher and incremental AST/Knowledge
updates; explicit ` + brand.BinName() + ` sync and direct index commands remain available.
The runtime key is regenerated on each daemon start.

Lifecycle:
  ` + brand.BinName() + ` daemon                          Start in foreground (Ctrl+C to stop)
  ` + brand.BinName() + ` daemon --ui                     Start in foreground with the UI
  ` + brand.BinName() + ` daemon stop                      Stop the running daemon
  ` + brand.BinName() + ` daemon status                    Show daemon health
  ` + brand.BinName() + ` daemon restart                   Restart through the OS manager

Managed service (current user):
  ` + brand.BinName() + ` daemon service start             Start through the OS manager
  ` + brand.BinName() + ` daemon service install --login   Also start at user login
  ` + brand.BinName() + ` daemon service login disable    Disable login startup only
  ` + brand.BinName() + ` daemon service stop              Stop without automatic relaunch
  ` + brand.BinName() + ` daemon service status            Show service and login state
  ` + brand.BinName() + ` daemon service uninstall         Stop and remove the service

The managed daemon always serves the UI itself. --managed is an internal flag
used by the OS service, not needed for foreground use. The tray opens the
published UI URL in your browser; it does not start a second UI server.

PID file: ~/` + brand.DotDir() + `/daemon/daemon.pid (global, one daemon per machine)`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDaemonStart(noEmbedding, noDream, serveUI, logPath, managed)
		},
	}

	cmd.Flags().BoolVar(&noEmbedding, "no-embedding", false, "Disable background embedding module")
	cmd.Flags().BoolVar(&noDream, "no-dream", false, "Disable autonomous dream module")
	cmd.Flags().BoolVar(&serveUI, "ui", false, "Serve the UI from this daemon process")
	cmd.Flags().StringVar(&logPath, "log", "", "Log file path (default: ~/"+brand.DotDir()+"/daemon/daemon.log)")
	cmd.Flags().BoolVar(&managed, "managed", false, "Internal OS service mode")
	_ = cmd.Flags().MarkHidden("managed")

	cmd.AddCommand(
		newDaemonStopCmd(),
		newDaemonStatusCmd(),
		newDaemonRestartCmd(),
		newDaemonServiceCmd(),
	)

	return cmd
}

func runDaemonStart(noEmbedding, noDream, serveUI bool, logPath string, managed bool) error {
	closeMCP, err := runDaemonCore(noEmbedding, noDream, serveUI, logPath, managed)
	if !errors.Is(err, daemon.ErrReplace) {
		return err
	}
	closeMCP()
	return finishDaemonReplacement(managed, func() error {
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
		if serveUI {
			argv = append(argv, "--ui")
		}
		if logPath != "" {
			argv = append(argv, "--log", logPath)
		}
		return spawnDetachedDaemon(exe, argv)
	})
}

func finishDaemonReplacement(managed bool, spawn func() error) error {
	if managed {
		return fmt.Errorf("managed daemon requested replacement: %w", daemon.ErrReplace)
	}
	if spawnErr := spawn(); spawnErr != nil {
		return fmt.Errorf("spawning new daemon: %w", spawnErr)
	}
	return nil
}

func daemonShouldServeUI(managed, serveUI bool) bool {
	return managed || serveUI || config.DaemonServesUI(nil, nil)
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

func runDaemonCore(noEmbedding, noDream, serveUI bool, logPath string, managed bool) (closeMCP func(), err error) {
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
	var localONNXTasks []ai.ModelTask
	if !cfg.DisableEmbedding {
		localONNXTasks = append(localONNXTasks, ai.ModelTaskEmbedding)
	}
	if config.SearchRerank() {
		localONNXTasks = append(localONNXTasks, ai.ModelTaskRerank)
	}
	if err := ai.ValidateConfiguredLocalONNXDevices(localONNXTasks...); err != nil {
		return closeMCP, err
	}

	var sharedEmbedClient ai.EmbeddingClient
	if !cfg.DisableEmbedding {
		sharedEmbedClient = ai.NewLazyEmbeddingClient()
	}
	go func() { _ = ai.PrefetchConfiguredLocalModels(context.Background()) }()

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

		apiKey, genErr := resolveDaemonMCPAPIKey()
		if genErr != nil {
			return
		}

		mcpHandler := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
			return mcpstdio.NewServerForProfile(r.Header.Get(agentpolicy.ProfileHeader))
		}, nil)

		mcpMux := newDaemonMCPMux(mcpHandler, daemonMCPMuxOptions{
			RuntimeKey:     apiKey,
			Resolver:       auth.NewProtectedResourceResolver(),
			AllowedOrigins: config.ResolveMCPAllowedOrigins(nil, nil),
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
		if err := writeDaemonMCPKey(mcpKeyFile, apiKey); err != nil {
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
					if !daemonMCPKeyFileMatches(mcpKeyFile, apiKey) {
						_ = writeDaemonMCPKey(mcpKeyFile, apiKey)
					}
				}
			}
		}()

		close(mcpReady)

		httpServer := &http.Server{Handler: mcpMux}
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
			userMemoryURI := memory.TableURIFor("user", userID)
			if userMemoryURI != "" {
				d.AddGlobalModule(daemon.NewMemoryMaintenanceModule(userMemoryURI, 15*time.Minute))
				if sharedEmbedClient != nil {
					d.AddGlobalModule(daemon.NewMemoryEmbeddingModule(userMemoryURI, sharedEmbedClient, 2*time.Minute))
				}
			}
		} else if userErr != nil {
			p.Warn("user memory maintenance is disabled: %v", userErr)
		}
	}

	if daemonShouldServeUI(managed, serveUI) {
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

// daemonMCPMuxOptions carries what the MCP listener needs beyond the protocol handler:
// how to authenticate a caller, what to advertise to one that cannot yet authenticate,
// and which browser origins may reach it.
type daemonMCPMuxOptions struct {
	RuntimeKey     string
	Resolver       *auth.ProtectedResourceResolver
	Verifier       daemonAccessTokenVerifier
	AllowedOrigins []string
}

func newDaemonMCPMux(mcpHandler http.Handler, opts daemonMCPMuxOptions) http.Handler {
	mux := http.NewServeMux()
	corsPolicy := newMCPCORS(opts.AllowedOrigins)

	mux.Handle("/mcp", corsPolicy(&mcpBearerHandler{
		next:       mcpHandler,
		runtimeKey: opts.RuntimeKey,
		resolver:   opts.Resolver,
		verifier:   opts.Verifier,
	}))

	// RFC 9728 locates metadata under the well-known prefix, optionally suffixed with the
	// resource's own path. Clients disagree on which one they request, so both answer.
	metadata := mcpMetadataHandler(opts.Resolver)
	mux.Handle("/.well-known/oauth-protected-resource", metadata)
	mux.Handle("/.well-known/oauth-protected-resource/", metadata)

	// Health stays unauthenticated: the container image's health check calls it, and it
	// reveals nothing beyond the process being up.
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status":"ok"}`))
	})
	return mux
}

func resolveDaemonMCPAPIKey() (string, error) {
	return mcpproxy.GenerateAPIKey()
}

type daemonAccessTokenVerifier interface {
	VerifyAccessToken(context.Context, auth.Provider, string, []string) (auth.VerifiedIdentity, error)
}

// acceptedAudiences carries the `aud` values this endpoint publishes as its own, resolved from
// whatever the active provider advertises. It is additive: each provider type still contributes
// the audience its own configuration defines.
func daemonBearerContextWithVerifier(ctx context.Context, bearer, runtimeKey string, verifier daemonAccessTokenVerifier, acceptedAudiences []string) (context.Context, bool) {
	if secretEqual(bearer, runtimeKey) {
		return ctx, true
	}
	// An inbound bearer belongs to the caller, not to the daemon's own login.
	// Loading the stored provider must not refresh an unrelated (possibly expired)
	// daemon session before the caller's token can be verified.
	store, err := auth.Open()
	if err != nil {
		return ctx, false
	}
	snapshot, err := store.Active()
	if err != nil {
		return ctx, false
	}
	if snapshot.Provider.Type == auth.ProviderLocal {
		return ctx, secretEqual(bearer, snapshot.Profile.MCPKey)
	}
	if snapshot.Provider.Type != auth.ProviderOIDC && snapshot.Provider.Type != auth.ProviderBroker {
		return ctx, false
	}
	audiences := append([]string(nil), acceptedAudiences...)
	if snapshot.Provider.OIDC != nil {
		// A direct OIDC provider names its own audience; the resource it publishes arrives
		// through acceptedAudiences.
		audiences = append(audiences, snapshot.Provider.OIDC.MCPAudience)
	}
	identity, err := verifier.VerifyAccessToken(ctx, snapshot.Provider, bearer, audiences)
	if err != nil {
		return ctx, false
	}
	ctx = auth.WithBrokerBearer(ctx, bearer)
	ctx = auth.WithRequestIdentity(ctx, identity)
	ctx, err = hubaccess.WithTrustedSubject(ctx, hubaccess.Subject{UserID: identity.Username, TeamIDs: identity.Teams})
	if err != nil {
		return ctx, false
	}
	return ctx, true
}

func secretEqual(got, want string) bool {
	if got == "" || want == "" || len(got) != len(want) {
		return false
	}
	return subtle.ConstantTimeCompare([]byte(got), []byte(want)) == 1
}

func writeDaemonMCPKey(path, apiKey string) error {
	if err := os.WriteFile(path, []byte(apiKey), 0o600); err != nil {
		return err
	}
	return os.Chmod(path, 0o600)
}

func daemonMCPKeyFileMatches(path, apiKey string) bool {
	data, err := os.ReadFile(path)
	if err != nil || string(data) != apiKey {
		return false
	}
	if runtime.GOOS == "windows" {
		return true
	}
	info, err := os.Stat(path)
	return err == nil && info.Mode().Perm() == 0o600
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
	disableTask := config.IsModuleDisabled("task", nil, projectCfg)
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
		var lockfileAgents []string
		if lf != nil {
			lockfileAgents = lf.Agents
		}
		agent := config.ResolveProjectAgent("", nil, projectCfg, lockfileAgents)
		modules = append(modules, daemon.NewDreamModule(projectDir, agent))
	}
	if !disableMemory && lf != nil && lf.Project.ID != "" {
		projectMemoryURI := memory.TableURIFor("project", lf.Project.ID)
		if projectMemoryURI != "" {
			modules = append(modules, daemon.NewMemoryMaintenanceModule(projectMemoryURI, 15*time.Minute))
			if !disableEmbedding && sharedEmbedClient != nil {
				modules = append(modules, daemon.NewMemoryEmbeddingModule(projectMemoryURI, sharedEmbedClient, 2*time.Minute))
			}
		}
	}
	if !disableTask && lf != nil && lf.Project.ID != "" {
		modules = append(modules, daemon.NewTaskMaintenanceModule(projectDir, 15*time.Minute))
	}

	return modules, nil, nil
}

func newDaemonStopCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "stop",
		Short: "Stop the running global daemon",
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := daemonctl.Stop(); err != nil {
				return err
			}
			output.NewPrinter("").Success("Daemon stopped")
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
				if service, err := daemonservice.GetStatus(); err == nil {
					p.KeyValue("Service", formatServiceStatus(service))
				}
				p.Step("Start one with: %s daemon service start", brand.BinName())
				return nil
			}

			uptime := time.Since(alive.StartedAt).Truncate(time.Second)
			p.Header("Daemon Status")
			p.KeyValue("PID", fmt.Sprintf("%d", alive.PID))
			p.KeyValue("Started", alive.StartedAt.Format(time.RFC3339))
			p.KeyValue("Uptime", uptime.String())
			p.KeyValue("PID File", pid.Path())
			p.KeyValue("Scope", "global (all projects)")
			if service, err := daemonservice.GetStatus(); err == nil {
				p.KeyValue("Service", formatServiceStatus(service))
			}

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
	return newDaemonRestartCmdWithEnsure(daemon.EnsureRunning)
}

func newDaemonRestartCmdWithEnsure(ensureRunning func() (bool, error)) *cobra.Command {
	return &cobra.Command{
		Use:   "restart",
		Short: "Stop and restart the global daemon",
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			if err := daemonctl.Stop(); err != nil {
				return fmt.Errorf("stopping daemon: %w", err)
			}
			p.Running("Starting managed daemon…")
			if _, err := ensureRunning(); err != nil {
				return fmt.Errorf("starting managed daemon: %w", err)
			}
			p.Success("Daemon restarted")
			return nil
		},
	}
}

func formatServiceStatus(status daemonservice.Status) string {
	return status.String()
}

func newDaemonServiceCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "service",
		Aliases: []string{"scheduler"},
		Short:   "Manage the per-user OS supervised daemon",
		Long: `Manage the daemon through the current user's OS service manager.
An eligible command installs the service on first use and starts it without
enabling login start. Choose --login to start it automatically at future logins.
The daemon restarts after an unexpected failure while the service is active.`,
	}
	loginCmd := &cobra.Command{Use: "login", Short: "Manage service startup at user login"}
	loginCmd.AddCommand(
		&cobra.Command{Use: "enable", Short: "Start daemon and tray at user login", RunE: func(cmd *cobra.Command, args []string) error {
			if err := daemonservice.SetLogin(true); err != nil {
				return err
			}
			return tray.InstallLogin()
		}},
		&cobra.Command{Use: "disable", Short: "Disable daemon and tray startup at login", RunE: func(cmd *cobra.Command, args []string) error {
			if err := daemonservice.SetLogin(false); err != nil {
				return err
			}
			return tray.RemoveLogin()
		}},
		&cobra.Command{Use: "status", Short: "Show daemon and tray login settings", RunE: func(cmd *cobra.Command, args []string) error {
			status, err := daemonservice.GetStatus()
			if err != nil {
				return err
			}
			trayEnabled, err := tray.LoginEnabled()
			if err != nil {
				return err
			}
			_, err = fmt.Fprintf(cmd.OutOrStdout(), "Daemon at login: %t\nTray at login: %t\n", status.AutoStart, trayEnabled)
			return err
		}},
	)
	var login bool
	installCmd := &cobra.Command{
		Use:   "install",
		Short: "Register the per-user daemon service",
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := daemonservice.Install(login); err != nil {
				return err
			}
			if _, err := daemonctl.EnsureRunning(); err != nil {
				return err
			}
			if login {
				if err := tray.InstallLogin(); err != nil {
					return fmt.Errorf("enabling tray at login: %w", err)
				}
			} else if err := tray.RemoveLogin(); err != nil {
				return fmt.Errorf("disabling tray at login: %w", err)
			}
			_ = tray.EnsureRunning()
			output.NewPrinter("").Success("Daemon service installed and started")
			return nil
		},
	}
	installCmd.Flags().BoolVar(&login, "login", false, "Start the daemon at each user login")
	cmd.AddCommand(
		installCmd,
		loginCmd,
		&cobra.Command{Use: "start", Short: "Start the managed daemon", RunE: func(cmd *cobra.Command, args []string) error {
			_, err := daemonctl.EnsureRunning()
			if err == nil {
				_ = tray.EnsureRunning()
			}
			return err
		}},
		&cobra.Command{Use: "stop", Short: "Stop the managed daemon", RunE: func(cmd *cobra.Command, args []string) error {
			return daemonctl.Stop()
		}},
		&cobra.Command{Use: "restart", Short: "Restart the managed daemon", RunE: func(cmd *cobra.Command, args []string) error {
			return daemonctl.Restart()
		}},
		&cobra.Command{Use: "uninstall", Aliases: []string{"remove"}, Short: "Stop and remove the per-user daemon service and login startup", RunE: func(cmd *cobra.Command, args []string) error {
			if err := daemonctl.Stop(); err != nil {
				return err
			}
			if err := daemonservice.Remove(); err != nil {
				return err
			}
			return tray.RemoveLogin()
		}},
		&cobra.Command{Use: "status", Short: "Show service and login state", RunE: func(cmd *cobra.Command, args []string) error {
			status, err := daemonservice.GetStatus()
			if err != nil {
				return err
			}
			output.NewPrinter("").KeyValue("Service", formatServiceStatus(status))
			return nil
		}},
	)
	return cmd
}
