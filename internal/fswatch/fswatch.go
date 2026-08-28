// Package fswatch reports source-file changes from the operating system's own
// notification API instead of polling.
//
// The previous approach ran `git status --porcelain` on a timer, which cost a
// full worktree walk per tick per project and detected a change up to ~6 s late.
// Watching the filesystem is near-instant and idle-free, and — because it names
// the exact paths that changed — it lets the indexer skip discovery entirely
// (ast.RunPipelineForPaths), which alone measured ~350 ms of a ~1.07 s
// incremental on a 35k-file repository.
//
// Ignore rules are honoured at BOTH levels: an ignored directory never gets a
// watch registered (on Linux each watched directory costs an inotify watch, so
// this is what keeps the budget sane), and any event that slips through for an
// ignored path is dropped. The caller supplies the IgnoreChecker, so whichever
// custom ignore file a module uses — .astignore, .wikiignore, .knowledgeignore —
// applies automatically alongside .gitignore.
package fswatch

import (
	"context"
	"fmt"
	"io/fs"
	"log/slog"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"

	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// Batch is a coalesced set of changes. Paths are absolute.
type Batch struct {
	// Changed lists created or modified files.
	Changed []string
	// Removed lists deleted or renamed-away files.
	Removed []string
	// Rescan is set when events were lost (kernel queue overflow) and the caller
	// must fall back to a full scan to regain consistency. Changed/Removed are
	// then only a partial picture.
	Rescan bool
}

// Config configures a Watcher.
// Ignorer is the share of ignorer's contract a watcher needs — in fact the same
// contract: a directory's ignore files must be brought in as the tree is
// crossed, which is exactly ignorer.DirScope. An alias, an interface with the
// same methods, would need covariant method signatures to share At, which Go
// does not do.
type Ignorer = ignorer.DirScope

type Config struct {
	// Root is the directory tree to watch.
	Root string

	// Ignore decides which paths are skipped. Directories it rejects are never
	// watched at all. May be nil to watch everything.
	//
	// An interface rather than *ignorer.IgnoreChecker because one watcher can
	// feed consumers with different rules: the daemon drives both the AST index
	// and the wiki from a single watch, and each honours its own ignore file.
	// What gets watched has to be the union of what they care about, which no
	// single checker expresses.
	Ignore Ignorer

	// Accept optionally filters files by name (e.g. only extensions a parser
	// handles). Directories are never passed to it.
	Accept func(path string) bool

	// Debounce is how long to wait for quiet before emitting a batch, so a
	// save-storm or a branch checkout collapses into one reindex.
	Debounce time.Duration

	// MaxDebounce caps how long a batch may be held while events keep arriving,
	// so a continuously-busy tree still makes progress.
	MaxDebounce time.Duration

	Logger *slog.Logger
}

func (c *Config) applyDefaults() {
	if c.Debounce <= 0 {
		c.Debounce = 400 * time.Millisecond
	}
	if c.MaxDebounce <= 0 {
		c.MaxDebounce = 3 * time.Second
	}
}

// Watcher turns filesystem notifications into coalesced batches.
type Watcher struct {
	cfg        Config
	log        *slog.Logger
	fsw        *fsnotify.Watcher
	root       string
	watched    map[string]bool
	dirContext map[string]Ignorer
	mu         sync.Mutex
}

// New creates a Watcher over cfg.Root without starting it.
func New(cfg Config) (*Watcher, error) {
	cfg.applyDefaults()
	root, err := filepath.Abs(cfg.Root)
	if err != nil {
		return nil, fmt.Errorf("fswatch: resolve root: %w", err)
	}
	fsw, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, fmt.Errorf("fswatch: create watcher: %w", err)
	}
	return &Watcher{
		cfg:     cfg,
		log:     slogutil.Resolve(cfg.Logger),
		fsw:     fsw,
		root:    root,
		watched: make(map[string]bool),
	}, nil
}

// Close releases the watcher's OS resources.
func (w *Watcher) Close() error { return w.fsw.Close() }

// Start registers watches over the tree and streams coalesced batches until ctx
// is cancelled. The returned channel is closed when watching stops.
func (w *Watcher) Start(ctx context.Context) (<-chan Batch, error) {
	if err := w.addTree(w.root); err != nil {
		_ = w.fsw.Close()
		return nil, err
	}
	w.log.Info("fswatch started", "root", w.root, "watched_dirs", len(w.watched))

	out := make(chan Batch)
	go w.loop(ctx, out)
	return out, nil
}

// addTree registers a watch on dir and every non-ignored directory beneath it.
//
// The ignore context deepens monotonically with each directory crossed: a
// subdirectory's own .gitignore/.astignore applies to everything below it, which
// is what git does — so the checker used for a child is the one At()'d into the
// parent. Each watched directory's context is remembered for the accept() path,
// which has no tree to walk.
func (w *Watcher) addTree(dir string) error {
	return filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil // unreadable subtree: skip rather than abort the whole watch
		}
		if !d.IsDir() {
			return nil
		}
		rel, relErr := filepath.Rel(w.root, p)
		if relErr != nil {
			return nil
		}
		if w.isIgnoredDir(rel) {
			return filepath.SkipDir
		}
		if err := w.addWatch(p); err != nil {
			return err
		}
		if rel != "." && usableOK(w.cfg.Ignore) {
			if sub, ok := usable(w.cfg.Ignore.At(rel)); ok {
				w.mu.Lock()
				if w.dirContext == nil {
					w.dirContext = make(map[string]Ignorer)
				}
				w.dirContext[p] = sub
				w.mu.Unlock()
			}
		}
		return nil
	})
}

// usableOK is usable's nil-interface form, for callers that only need to know
// whether there are any rules at all.
func usableOK(s Ignorer) bool {
	_, ok := usable(s)
	return ok
}

// usable unwraps the no-rules case that Go's interfaces make invisible: a nil
// implemented as a nil *IgnoreChecker stored in the interface is NOT a nil
// interface, so a plain `== nil` check misses it. Every IsIgnored on the
// checker itself guards against the same receiver, but At does not — there is
// no rule to descend into, so callers need the boolean.
func usable(s Ignorer) (Ignorer, bool) {
	if s == nil {
		return nil, false
	}
	if c, ok := s.(*ignorer.IgnoreChecker); ok && c == nil {
		return nil, false
	}
	return s, true
}

func (w *Watcher) isIgnoredDir(rel string) bool {
	if rel == "." || rel == "" {
		return false
	}
	base := filepath.Base(rel)
	// .git churns constantly and holds nothing we index.
	if base == ".git" {
		return true
	}
	ctx, ok := usable(w.cfg.Ignore)
	if !ok {
		return false
	}
	// The caller passes the context of the DIRECTORY'S PARENT (the walk has not
	// crossed into rel yet); for the root's children there is no At yet and the
	// root rules are exactly right.
	if parentRel := path.Dir(rel); parentRel != "." {
		sub, ok := usable(ctx.At(parentRel))
		if !ok {
			return false
		}
		ctx = sub
	}
	return ctx.IsIgnored(rel, true) && !ctx.ShouldDescend(rel)
}

func (w *Watcher) addWatch(dir string) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.watched[dir] {
		return nil
	}
	if err := w.fsw.Add(dir); err != nil {
		// The most common failure by far is exhausting the per-user inotify
		// watch limit; say so explicitly instead of surfacing "no space left on
		// device", which sends people looking at disk usage.
		if strings.Contains(err.Error(), "no space left on device") ||
			strings.Contains(err.Error(), "too many open files") {
			return fmt.Errorf("fswatch: out of filesystem watches while adding %s: raise the limit, e.g. "+
				"sudo sysctl fs.inotify.max_user_watches=524288 (and fs.inotify.max_user_instances): %w", dir, err)
		}
		return fmt.Errorf("fswatch: watch %s: %w", dir, err)
	}
	w.watched[dir] = true
	return nil
}

// accept reports whether a file path should produce an event.
func (w *Watcher) accept(path string) bool {
	rel, err := filepath.Rel(w.root, path)
	if err != nil {
		return false
	}
	if strings.HasPrefix(rel, ".git/") || rel == ".git" {
		return false
	}
	if w.cfg.Ignore != nil {
		// The rules in force depend on WHERE the file is: a subdirectory's own
		// .gitignore/.astignore applies to everything below it, so the event is
		// judged by the context its directory was registered with — or, for a
		// path whose directory we never watched (delayed event race), by the
		// context computed from the tree we do know, the root rules.
		if ctx, ok := usable(w.cfg.Ignore); ok {
			if dirAbs := filepath.Dir(path); dirAbs != w.root {
				w.mu.Lock()
				cached := w.dirContext[dirAbs]
				w.mu.Unlock()
				if c, ok := usable(cached); ok {
					ctx = c
				}
			}
			if ctx.IsIgnored(rel, false) {
				return false
			}
		}
	}
	if w.cfg.Accept != nil && !w.cfg.Accept(path) {
		return false
	}
	return true
}

// loop coalesces raw events into batches.
func (w *Watcher) loop(ctx context.Context, out chan<- Batch) {
	defer close(out)

	changed := make(map[string]bool)
	removed := make(map[string]bool)
	rescan := false

	var (
		timer    *time.Timer
		timerC   <-chan time.Time
		deadline time.Time
	)
	arm := func() {
		now := time.Now()
		if timer == nil {
			deadline = now.Add(w.cfg.MaxDebounce)
			timer = time.NewTimer(w.cfg.Debounce)
			timerC = timer.C
			return
		}
		// Extend the quiet window, but never past MaxDebounce.
		wait := w.cfg.Debounce
		if rest := time.Until(deadline); rest < wait {
			wait = rest
		}
		if wait < 0 {
			wait = 0
		}
		if !timer.Stop() {
			select {
			case <-timer.C:
			default:
			}
		}
		timer.Reset(wait)
	}
	flush := func() {
		batch := Batch{Rescan: rescan}
		for p := range changed {
			batch.Changed = append(batch.Changed, p)
		}
		for p := range removed {
			batch.Removed = append(batch.Removed, p)
		}
		changed = make(map[string]bool)
		removed = make(map[string]bool)
		rescan = false
		timer, timerC = nil, nil

		if len(batch.Changed) == 0 && len(batch.Removed) == 0 && !batch.Rescan {
			return
		}
		// Checked before the select: with a reader ready AND the context already
		// cancelled, both cases are ready and Go picks one at random, so a watcher
		// being shut down still handed a batch downstream about half the time.
		// Same shape that let sysutil.AcquireHeavy grant a slot to a cancelled
		// caller — there it started a full reindex, here it only queues work that
		// is about to be dropped, but the fix is the same and costs one branch.
		if ctx.Err() != nil {
			return
		}
		select {
		case out <- batch:
		case <-ctx.Done():
		}
	}

	for {
		select {
		case <-ctx.Done():
			return

		case ev, ok := <-w.fsw.Events:
			if !ok {
				return
			}
			w.handleEvent(ev, changed, removed)
			if len(changed) > 0 || len(removed) > 0 {
				arm()
			}

		case err, ok := <-w.fsw.Errors:
			if !ok {
				return
			}
			// A dropped-event error means the kernel queue overflowed and the
			// picture is incomplete; only a rescan restores consistency.
			w.log.Warn("fswatch error, requesting rescan", "error", err)
			rescan = true
			arm()

		case <-timerC:
			flush()
		}
	}
}

func (w *Watcher) handleEvent(ev fsnotify.Event, changed, removed map[string]bool) {
	// A new directory must be watched too, and files created inside it before
	// the watch landed would otherwise be missed — so scan it.
	if ev.Has(fsnotify.Create) {
		if info, err := filepath.Abs(ev.Name); err == nil {
			if isDir(info) {
				rel, relErr := filepath.Rel(w.root, info)
				if relErr == nil && !w.isIgnoredDir(rel) {
					if err := w.addTree(info); err != nil {
						w.log.Warn("fswatch: watch new directory", "dir", info, "error", err)
					}
					w.scanNewDir(info, changed)
				}
				return
			}
		}
	}

	switch {
	case ev.Has(fsnotify.Remove) || ev.Has(fsnotify.Rename):
		if w.accept(ev.Name) {
			removed[ev.Name] = true
			delete(changed, ev.Name)
		}
		w.mu.Lock()
		delete(w.watched, ev.Name)
		w.mu.Unlock()
	case ev.Has(fsnotify.Create) || ev.Has(fsnotify.Write):
		if w.accept(ev.Name) {
			changed[ev.Name] = true
			delete(removed, ev.Name)
		}
	}
}

// scanNewDir records files that already exist in a directory that was just
// created, closing the race between mkdir and the watch being registered.
func (w *Watcher) scanNewDir(dir string, changed map[string]bool) {
	_ = filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		if w.accept(p) {
			changed[p] = true
		}
		return nil
	})
}

// isDir reports whether path is a directory, tolerating a vanished path.
func isDir(path string) bool {
	fi, err := os.Stat(path)
	return err == nil && fi.IsDir()
}
