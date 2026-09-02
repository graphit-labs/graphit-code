package daemon

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
)

var ErrReplace = errors.New("daemon: replacement required")

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

	// ProjectActivityWindow bounds how long a registered project may go
	// without a filesystem change before its supervisor is parked (fs watch,
	// embedding loop and dream runner all stopped). Zero disables parking —
	// every registered project stays supervised for as long as it stays
	// registered. Resolved from daemon.activity_window by the CLI layer;
	// left at zero here so a Daemon built directly (as tests do) keeps the
	// pre-parking behavior.
	ProjectActivityWindow time.Duration

	OnEvent func(level string, msg string)
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
	cfg         Config
	pid         *PIDFile
	builder     ProjectModuleBuilder
	supervisors map[string]*ProjectSupervisor
	// parked holds registered projects that are not currently supervised —
	// either newly discovered but quiet, or demoted after their supervisor's
	// IdleFor() exceeded cfg.ProjectActivityWindow. Only populated when that
	// window is non-zero. Guarded by mu, same as supervisors.
	parked    map[string]ProjectInfo
	logFile   *os.File
	mu        sync.RWMutex // protects supervisors and parked maps
	logMu     sync.Mutex   // protects logFile writes (separate to avoid deadlock)
	bootStamp string

	// grammarSigs records what each grammar directory looked like when this
	// process last accepted it: "" for the global pair, one entry per supervised
	// project. Guarded by mu.
	grammarSigs map[string]string

	// globalModules belong to the machine rather than to a project — the memory watcher,
	// whose one root covers every scope, and the embedding server, whose ONNX session every
	// process on the machine shares. They are supervised like a project's modules; see
	// SuperviseGlobal for why they used to not be.
	globalModules []WatchModule
}

// AddGlobalModule registers a module that outlives any single project. Call before Run.
func (d *Daemon) AddGlobalModule(mod WatchModule) {
	if mod != nil {
		d.globalModules = append(d.globalModules, mod)
	}
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
		parked:      make(map[string]ProjectInfo),
	}
}

func (d *Daemon) event(level string, format string, args ...any) {
	if d.cfg.OnEvent != nil {
		d.cfg.OnEvent(level, fmt.Sprintf(format, args...))
	}
}

func (d *Daemon) Start(ctx context.Context, discoverFn func() ([]ProjectInfo, error), onReady ...func()) error {
	chdirToStableDir()

	if err := d.pid.Acquire(); err != nil {
		if errors.Is(err, ErrAlreadyRunning) {
			if alive := d.pid.IsAlive(); alive != nil {
				return fmt.Errorf("daemon already running (pid %d, started %s)",
					alive.PID, alive.StartedAt.Format(time.RFC3339))
			}
			return fmt.Errorf("daemon already running (lock held)")
		}
		return fmt.Errorf("acquiring pid lock: %w", err)
	}
	defer d.pid.Release()

	d.bootStamp = readLauncherStamp()
	d.mu.Lock()
	d.grammarSigs = map[string]string{"": ast.GrammarSignature("")}
	d.mu.Unlock()

	if err := os.MkdirAll(filepath.Dir(d.cfg.LogPath), 0o755); err != nil {
		return fmt.Errorf("creating log dir: %w", err)
	}
	lf, err := os.OpenFile(d.cfg.LogPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return fmt.Errorf("opening log file: %w", err)
	}
	d.logFile = lf
	defer func() { _ = lf.Close() }()

	for _, fn := range onReady {
		fn()
	}

	d.log("daemon started (pid=%d, discovery_interval=%s, version_check=%s, stamp=%s)",
		os.Getpid(), d.cfg.DiscoveryInterval, d.cfg.VersionCheckInterval, d.bootStamp)

	for _, mod := range d.globalModules {
		go SuperviseGlobal(ctx, mod, d.log)
	}

	d.event("running", "Global daemon started")
	d.event("step", "Discovery interval: %s", d.cfg.DiscoveryInterval)
	d.event("step", "Press Ctrl+C to stop")
	d.event("blank", "")

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
				d.log("launcher stamp changed — shutting down for replacement")
				d.event("warn", "New version detected — replacing daemon process")
				d.shutdown()
				return ErrReplace
			}
			if where, changed := d.grammarsChanged(); changed {
				// Query files reload in place; grammar libraries cannot. A
				// *sitter.Language backs live parse state and is memoised for
				// the life of the process, so the only way to pick up a newly
				// installed one is the same exit the launcher already handles.
				d.log("grammar libraries changed in %s — shutting down for replacement", where)
				d.event("warn", "New grammar installed — replacing daemon process")
				d.shutdown()
				return ErrReplace
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
			d.event("step_warn", "Project removed: %s", sup.projectDir)
			sup.Stop()
			delete(d.supervisors, id)
		}
	}
	for id := range d.parked {
		if _, ok := discovered[id]; !ok {
			delete(d.parked, id)
		}
	}

	window := d.cfg.ProjectActivityWindow
	if window > 0 && d.parked == nil {
		d.parked = make(map[string]ProjectInfo)
	}

	// A supervised project that has gone quiet for longer than the activity
	// window is parked: its fs watch, embedding loop and dream runner all
	// stop, and it falls back to the periodic mtime probe below until it has
	// something to reindex again.
	if window > 0 {
		for id, sup := range d.supervisors {
			if idle := sup.IdleFor(); idle < window {
				continue
			}
			d.log("[%s] idle for %s (window %s) — parking supervisor: %s",
				id, sup.IdleFor().Round(time.Second), window, sup.projectDir)
			d.event("step_warn", "Project idle, parking: %s", sup.projectDir)
			sup.Stop()
			delete(d.supervisors, id)
			d.parked[id] = discovered[id]
		}
	}

	for id, proj := range discovered {
		if _, ok := d.supervisors[id]; ok {
			continue
		}

		_, wasParked := d.parked[id]

		if window > 0 && !d.projectRecentlyActive(proj.Dir, window) {
			d.parked[id] = proj
			continue
		}
		delete(d.parked, id)

		if wasParked {
			d.log("[%s] activity detected — resuming supervision: %s", id, proj.Dir)
			d.event("step_ok", "Project active again: %s", proj.Dir)
		} else {
			d.log("[%s] new project discovered: %s", id, proj.Dir)
			d.event("step_ok", "New project: %s", proj.Dir)
		}

		modules, closerFns, err := d.builder(proj.Dir)
		if err != nil {
			d.log("[%s] failed to build modules: %v", id, err)
			d.event("step_warn", "Failed to start project %s: %v", proj.Dir, err)
			continue
		}

		if len(modules) == 0 {
			d.log("[%s] no modules to supervise — skipping", id)
			continue
		}

		sup := newProjectSupervisor(id, proj.Dir, modules)
		for _, mod := range modules {
			if ar, ok := mod.(ActivityReporter); ok {
				ar.SetActivityCallback(sup.Touch)
			}
		}
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

// projectRecentlyActive reports whether dir has had a file touched (skipping
// .git and the brand directory — see dream.LastModifiedTime) within window.
// A walk failure — an inaccessible or momentarily empty tree — defaults to
// true, so a project is never parked on account of the probe itself failing.
func (d *Daemon) projectRecentlyActive(dir string, window time.Duration) bool {
	last, err := dream.LastModifiedTime(dir)
	if err != nil {
		return true
	}
	return time.Since(last) < window
}

func (d *Daemon) shutdown() {
	d.log("daemon shutting down…")
	d.event("warn", "Shutting down…")

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
		d.event("warn", "Shutdown timed out after 10s")
	}

	d.event("success", "Daemon stopped")

	hub.WaitForPendingEvents()

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

// grammarsChanged reports whether a grammar directory this process already
// accepted now looks different, and which one.
//
// A directory seen for the first time is recorded, not acted on: a project
// discovered with grammars already installed is not a reason to restart, and
// treating it as one would make the daemon bounce every time a new project
// appeared.
func (d *Daemon) grammarsChanged() (string, bool) {
	dirs := []string{""}
	d.mu.RLock()
	for _, sup := range d.supervisors {
		dirs = append(dirs, sup.projectDir)
	}
	d.mu.RUnlock()

	var changedIn string
	var changed bool

	d.mu.Lock()
	for _, dir := range dirs {
		sig := ast.GrammarSignature(dir)
		known, seen := d.grammarSigs[dir]
		if !seen {
			d.grammarSigs[dir] = sig
			continue
		}
		if sig != known {
			d.grammarSigs[dir] = sig
			if !changed {
				changedIn, changed = dir, true
				if changedIn == "" {
					changedIn = "the global grammar directory"
				}
			}
		}
	}
	d.mu.Unlock()

	return changedIn, changed
}

func (d *Daemon) log(format string, args ...any) {
	if d.logFile == nil {
		return
	}
	msg := fmt.Sprintf(format, args...)
	line := fmt.Sprintf("[%s] %s\n", time.Now().Format("2006-01-02 15:04:05"), msg)
	// Uses a dedicated logMu instead of d.mu to avoid deadlock:
	// reconcileProjects() holds d.mu.Lock() and calls d.log().
	d.logMu.Lock()
	defer d.logMu.Unlock()
	_, _ = d.logFile.WriteString(line)
}

type closerFunc func() error

func (f closerFunc) Close() error { return f() }

// chdirToStableDir moves the daemon off whatever directory spawned it.
//
// The daemon serves many projects and must depend on none of their directories, but it
// inherits the cwd of whoever started it — a CLI invocation from a checkout, or a test
// that chdir'd into its own t.TempDir(). Nothing brings that cwd back when the
// directory is deleted, and the daemon outlives the thing that spawned it by design.
//
// The failure that follows is nasty because it is partial: only handlers that call
// os.Getwd() break, with "getwd: no such file or directory", while every handler that
// resolves from an explicit project_dir keeps working. That makes the breakage look
// like it belongs to whichever module happened to use one. It was observed after a
// full test run, with the daemon's cwd pointing at a removed t.TempDir().
//
// Best effort on purpose: a daemon that cannot chdir is still a working daemon, and
// refusing to start over it would turn a latent problem into an outage.
func chdirToStableDir() {
	dir := brand.GlobalDir()
	if dir == "" {
		return
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return
	}
	_ = os.Chdir(dir)
}
