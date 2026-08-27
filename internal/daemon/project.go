package daemon

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime/debug"
	"sync"
	"sync/atomic"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ActivityReporter is implemented by modules that can tell their supervisor
// about filesystem activity as they see it — SyncModule, watching every file
// change in the project — so the supervisor's idle clock resets on real
// activity instead of daemon.reconcileProjects having to re-walk the tree.
type ActivityReporter interface {
	SetActivityCallback(func())
}

type ProjectSupervisor struct {
	projectDir     string
	projectID      string
	modules        []*moduleEntry
	closers        []io.Closer
	cancel         context.CancelFunc
	mu             sync.RWMutex // protects stopped, cancel
	logMu          sync.Mutex   // protects projectLogFile writes (separate to avoid deadlock)
	stopped        bool
	projectLogFile *os.File
	globalLogFn    func(string, ...any)

	// lastActivity is a UnixNano timestamp, touched by modules implementing
	// ActivityReporter whenever they observe a filesystem change. reconcileProjects
	// parks a supervisor once IdleFor() exceeds the configured activity window.
	lastActivity atomic.Int64
}

func newProjectSupervisor(projectID, projectDir string, modules []WatchModule) *ProjectSupervisor {
	entries := make([]*moduleEntry, 0, len(modules))
	for _, m := range modules {
		entries = append(entries, newModuleEntry(m))
	}
	ps := &ProjectSupervisor{
		projectDir: projectDir,
		projectID:  projectID,
		modules:    entries,
	}
	ps.Touch()
	return ps
}

// Touch records filesystem activity, resetting the idle clock.
func (ps *ProjectSupervisor) Touch() {
	ps.lastActivity.Store(time.Now().UnixNano())
}

// IdleFor reports how long it has been since the last recorded activity.
func (ps *ProjectSupervisor) IdleFor() time.Duration {
	return time.Since(time.Unix(0, ps.lastActivity.Load()))
}

func (ps *ProjectSupervisor) AddCloser(c io.Closer) {
	ps.closers = append(ps.closers, c)
}

func (ps *ProjectSupervisor) Start(ctx context.Context, logFn func(string, ...any)) {
	ctx, ps.cancel = context.WithCancel(ctx)
	ps.globalLogFn = logFn

	projectLogDir := brand.ProjectRuntimePath(ps.projectDir, "daemon")
	if err := os.MkdirAll(projectLogDir, 0o755); err == nil {
		if f, err := os.OpenFile(
			filepath.Join(projectLogDir, "daemon.log"),
			os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644,
		); err == nil {
			ps.projectLogFile = f
		}
	}
	ps.projectLog("supervisor started with %d module(s)", len(ps.modules))

	var wg sync.WaitGroup
	for _, entry := range ps.modules {
		wg.Add(1)
		go func(e *moduleEntry) {
			defer wg.Done()
			ps.supervise(ctx, e)
		}(entry)
	}

	<-ctx.Done()

	ps.projectLog("supervisor shutting down")

	for _, c := range ps.closers {
		if err := c.Close(); err != nil {
			ps.projectLog("closer error: %v", err)
		}
	}

	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()

	select {
	case <-done:
		ps.projectLog("all modules stopped gracefully")
	case <-time.After(7 * time.Second):
		ps.projectLog("module shutdown timed out after 7s")
	}

	if ps.projectLogFile != nil {
		_ = ps.projectLogFile.Close()
	}
}

func (ps *ProjectSupervisor) Stop() {
	ps.mu.Lock()
	defer ps.mu.Unlock()
	if ps.cancel != nil {
		ps.cancel()
	}
	ps.stopped = true
}

func (ps *ProjectSupervisor) projectLog(format string, args ...any) {
	msg := fmt.Sprintf(format, args...)
	line := fmt.Sprintf("[%s] %s\n", time.Now().Format("2006-01-02 15:04:05"), msg)

	ps.logMu.Lock()
	defer ps.logMu.Unlock()
	if ps.projectLogFile != nil {
		_, _ = ps.projectLogFile.WriteString(line)
	}

	if ps.globalLogFn != nil {
		ps.globalLogFn("[%s] %s", ps.projectID, msg)
	}
}

func (ps *ProjectSupervisor) supervise(ctx context.Context, entry *moduleEntry) {
	modName := entry.mod.Name()

	for {
		if ctx.Err() != nil {
			entry.setState(ModuleStopped)
			return
		}

		modCtx, modCancel := context.WithCancel(ctx)
		entry.mu.Lock()
		entry.cancel = modCancel
		entry.mu.Unlock()

		entry.setStarted()
		startTime := time.Now()

		ps.projectLog("%s: starting", modName)

		err := runProtected(modCtx, entry)
		modCancel()

		if ctx.Err() != nil {
			entry.setState(ModuleStopped)
			ps.projectLog("%s: stopped (shutdown)", modName)
			return
		}

		entry.setState(ModuleCrashed)
		entry.setError(err)
		restarts := entry.incRestarts()

		ps.projectLog("%s: crashed (attempt=%d, error=%v)", modName, restarts, err)

		if time.Since(startTime) >= stableAfter {
			entry.resetRestarts()
			restarts = 0
			ps.projectLog("%s: was stable for >%s, reset restart counter", modName, stableAfter)
		}

		if restarts >= maxRestarts {
			entry.setState(ModuleFailed)
			ps.projectLog("%s: FAILED — exceeded max restarts (%d)", modName, maxRestarts)
			return
		}

		backoff := time.Duration(1<<uint(restarts)) * time.Second
		if backoff > maxBackoff {
			backoff = maxBackoff
		}
		ps.projectLog("%s: backing off %s before restart", modName, backoff)

		select {
		case <-ctx.Done():
			entry.setState(ModuleStopped)
			return
		case <-time.After(backoff):
		}
	}
}

func runProtected(ctx context.Context, entry *moduleEntry) (retErr error) {
	defer func() {
		if r := recover(); r != nil {
			// The stack, not just the value: a module that crash-loops writes one
			// line per restart, and "panic: slice bounds out of range" names no
			// file, no function and no line. Sixty-six of those accumulated over
			// twelve days here without ever saying where to look.
			retErr = fmt.Errorf("panic: %v\n%s", r, debug.Stack())
		}
	}()
	return entry.mod.Start(ctx)
}
