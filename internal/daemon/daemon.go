package daemon

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
)

const (
	maxRestarts = 10

	stableAfter = 60 * time.Second

	maxBackoff = 30 * time.Second

	defaultVersionCheckInterval = 30 * time.Second
)

type Config struct {
	LogPath string

	DiscoveryInterval time.Duration

	VersionCheckInterval time.Duration

	DisableEmbedding bool

	DisableDream bool
}

func GlobalDaemonDir() string {
	return filepath.Join(brand.GlobalDir(), "daemon")
}

func DefaultConfig() Config {
	return Config{
		DiscoveryInterval:    30 * time.Second,
		VersionCheckInterval: defaultVersionCheckInterval,
		LogPath:              filepath.Join(GlobalDaemonDir(), "daemon.log"),
	}
}

type ProjectInfo struct {
	ID  string
	Dir string
}

type ProjectModuleBuilder func(projectDir string) (modules []WatchModule, closers []func() error, err error)

type Daemon struct {
	cfg          Config
	pid          *PIDFile
	builder      ProjectModuleBuilder
	supervisors  map[string]*ProjectSupervisor
	printer      *output.Printer
	logFile      *os.File
	mu           sync.RWMutex
	bootStamp    string
	pidHandedOff bool
}

func New(cfg Config, builder ProjectModuleBuilder) *Daemon {
	if cfg.DiscoveryInterval <= 0 {
		cfg.DiscoveryInterval = 30 * time.Second
	}
	if cfg.VersionCheckInterval <= 0 {
		cfg.VersionCheckInterval = defaultVersionCheckInterval
	}
	return &Daemon{
		cfg:         cfg,
		pid:         NewPIDFile(),
		builder:     builder,
		supervisors: make(map[string]*ProjectSupervisor),
		printer:     output.NewPrinter("daemon"),
	}
}

func (d *Daemon) Start(ctx context.Context, discoverFn func() ([]ProjectInfo, error)) error {

	if alive := d.pid.IsAlive(); alive != nil {
		return fmt.Errorf("daemon already running (pid %d, started %s)",
			alive.PID, alive.StartedAt.Format(time.RFC3339))
	}

	d.bootStamp = readLauncherStamp()

	if err := os.MkdirAll(filepath.Dir(d.cfg.LogPath), 0o755); err != nil {
		return fmt.Errorf("creating log dir: %w", err)
	}
	lf, err := os.OpenFile(d.cfg.LogPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return fmt.Errorf("opening log file: %w", err)
	}
	d.logFile = lf
	defer func() { _ = lf.Close() }()

	if err := d.pid.Write(); err != nil {
		return fmt.Errorf("writing pid file: %w", err)
	}
	defer func() {

		if !d.pidHandedOff {
			d.pid.Remove()
		}
	}()

	d.log("daemon started (pid=%d, discovery_interval=%s, version_check=%s, stamp=%s)",
		os.Getpid(), d.cfg.DiscoveryInterval, d.cfg.VersionCheckInterval, d.bootStamp)

	d.printer.Running("Global daemon started")
	d.printer.Step("Discovery interval: %s", d.cfg.DiscoveryInterval)
	d.printer.Step("Press Ctrl+C to stop")
	d.printer.Blank()

	d.reconcileProjects(ctx, discoverFn)

	discoveryTicker := time.NewTicker(d.cfg.DiscoveryInterval)
	defer discoveryTicker.Stop()

	versionTicker := time.NewTicker(d.cfg.VersionCheckInterval)
	defer versionTicker.Stop()

	for {
		select {
		case <-ctx.Done():
			d.shutdown()
			return nil
		case <-discoveryTicker.C:
			d.reconcileProjects(ctx, discoverFn)
		case <-versionTicker.C:
			if d.stampChanged() {
				d.log("launcher stamp changed — spawning new daemon before shutdown")
				d.printer.Warn("New version detected — upgrading daemon")

				d.pid.Remove()
				d.pidHandedOff = true

				started, err := EnsureRunning()
				if err != nil {
					d.log("failed to spawn replacement daemon: %v", err)
				} else if started {
					d.log("replacement daemon spawned successfully")
				}

				d.shutdown()
				return nil
			}
		}
	}
}

func (d *Daemon) reconcileProjects(ctx context.Context, discoverFn func() ([]ProjectInfo, error)) {
	projects, err := discoverFn()
	if err != nil {
		d.log("project discovery failed: %v", err)
		return
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	discovered := make(map[string]ProjectInfo, len(projects))
	for _, p := range projects {
		discovered[p.ID] = p
	}

	for id, sup := range d.supervisors {
		if _, ok := discovered[id]; !ok {
			d.log("[%s] project removed — stopping supervisor", id)
			d.printer.StepWarn("Project removed: %s", sup.projectDir)
			sup.Stop()
			delete(d.supervisors, id)
		}
	}

	for id, proj := range discovered {
		if _, ok := d.supervisors[id]; ok {
			continue
		}

		d.log("[%s] new project discovered: %s", id, proj.Dir)
		d.printer.StepOK("New project: %s", proj.Dir)

		modules, closerFns, err := d.builder(proj.Dir)
		if err != nil {
			d.log("[%s] failed to build modules: %v", id, err)
			d.printer.StepWarn("Failed to start project %s: %v", proj.Dir, err)
			continue
		}

		if len(modules) == 0 {
			d.log("[%s] no modules to supervise — skipping", id)
			continue
		}

		sup := newProjectSupervisor(id, proj.Dir, modules)
		for _, fn := range closerFns {
			sup.AddCloser(closerFunc(fn))
		}

		d.supervisors[id] = sup

		go func(s *ProjectSupervisor) {
			s.Start(ctx, d.log)
		}(sup)

		d.log("[%s] supervisor launched with %d module(s)", id, len(modules))
	}
}

func (d *Daemon) shutdown() {
	d.log("daemon shutting down…")
	d.printer.Warn("Shutting down…")

	d.mu.Lock()
	sups := make([]*ProjectSupervisor, 0, len(d.supervisors))
	for _, sup := range d.supervisors {
		sups = append(sups, sup)
	}
	d.mu.Unlock()

	var wg sync.WaitGroup
	for _, sup := range sups {
		wg.Add(1)
		go func(s *ProjectSupervisor) {
			defer wg.Done()
			s.Stop()
		}(sup)
	}

	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()

	select {
	case <-done:
		d.log("all project supervisors stopped gracefully")
	case <-time.After(10 * time.Second):
		d.log("shutdown timed out after 10s")
		d.printer.Warn("Shutdown timed out after 10s")
	}

	d.printer.Success("Daemon stopped")

	memory.WaitForPendingPushes()

	d.log("daemon stopped")
}

func (d *Daemon) stampChanged() bool {
	if d.bootStamp == "" {
		return false
	}
	current := readLauncherStamp()
	if current == "" {
		return false
	}
	return current != d.bootStamp
}

func (d *Daemon) log(format string, args ...any) {
	if d.logFile == nil {
		return
	}
	msg := fmt.Sprintf(format, args...)
	line := fmt.Sprintf("[%s] %s\n", time.Now().Format("2006-01-02 15:04:05"), msg)
	d.mu.RLock()
	_, _ = d.logFile.WriteString(line)
	d.mu.RUnlock()
}

type closerFunc func() error

func (f closerFunc) Close() error { return f() }
