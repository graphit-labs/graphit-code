package fswatch

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

func write(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// waitBatch collects batches until the predicate is satisfied or time runs out.
func waitBatch(t *testing.T, ch <-chan Batch, timeout time.Duration, ok func(Batch) bool) (Batch, bool) {
	t.Helper()
	deadline := time.After(timeout)
	for {
		select {
		case b, open := <-ch:
			if !open {
				return Batch{}, false
			}
			if ok(b) {
				return b, true
			}
		case <-deadline:
			return Batch{}, false
		}
	}
}

func contains(paths []string, suffix string) bool {
	for _, p := range paths {
		if strings.HasSuffix(filepath.ToSlash(p), suffix) {
			return true
		}
	}
	return false
}

func startWatcher(t *testing.T, root string, ic *ignorer.IgnoreChecker) <-chan Batch {
	t.Helper()
	w, err := New(Config{
		Root:        root,
		Ignore:      ic,
		Debounce:    60 * time.Millisecond,
		MaxDebounce: 500 * time.Millisecond,
	})
	if err != nil {
		t.Fatalf("new watcher: %v", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(func() { cancel(); _ = w.Close() })
	ch, err := w.Start(ctx)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	return ch
}

func TestDetectsCreateAndModify(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, "src", "a.go"), "package a\n")
	ch := startWatcher(t, root, nil)

	write(t, filepath.Join(root, "src", "b.go"), "package b\n")
	if _, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Changed, "src/b.go")
	}); !ok {
		t.Fatal("did not observe creation of src/b.go")
	}

	write(t, filepath.Join(root, "src", "a.go"), "package a // edited\n")
	if _, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Changed, "src/a.go")
	}); !ok {
		t.Fatal("did not observe modification of src/a.go")
	}
}

func TestDetectsRemoval(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(root, "gone.go")
	write(t, target, "package g\n")
	ch := startWatcher(t, root, nil)

	if err := os.Remove(target); err != nil {
		t.Fatal(err)
	}
	if _, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Removed, "gone.go")
	}); !ok {
		t.Fatal("did not observe removal")
	}
}

// TestRespectsGitignoreAndCustomIgnore is the core requirement: .gitignore and
// whatever custom ignore file the module uses (.astignore here; .wikiignore and
// .knowledgeignore work the same way) must both suppress events, and ignored
// directories must not even be watched.
func TestRespectsGitignoreAndCustomIgnore(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, ".gitignore"), "build/\n*.log\n")
	write(t, filepath.Join(root, ".astignore"), "vendor/\nsecret.go\n")
	write(t, filepath.Join(root, "src", "keep.go"), "package k\n")
	write(t, filepath.Join(root, "build", "out.go"), "package o\n")
	write(t, filepath.Join(root, "vendor", "dep.go"), "package d\n")

	ic := ignorer.New(root, root, ".astignore", nil)
	ch := startWatcher(t, root, ic)

	// Ignored writes first, then an accepted one. The accepted event acts as a
	// barrier: once it arrives, any ignored event would already have been
	// delivered had it not been filtered.
	write(t, filepath.Join(root, "build", "out.go"), "package o // changed\n")
	write(t, filepath.Join(root, "vendor", "dep.go"), "package d // changed\n")
	write(t, filepath.Join(root, "app.log"), "noise\n")
	write(t, filepath.Join(root, "secret.go"), "package s\n")
	write(t, filepath.Join(root, "src", "keep.go"), "package k // changed\n")

	b, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Changed, "src/keep.go")
	})
	if !ok {
		t.Fatal("did not observe the accepted file")
	}
	for _, bad := range []string{"build/out.go", "vendor/dep.go", "app.log", "secret.go"} {
		if contains(b.Changed, bad) || contains(b.Removed, bad) {
			t.Errorf("ignored path leaked into batch: %s (batch=%v)", bad, b.Changed)
		}
	}
}

// TestIgnoredDirectoriesAreNotWatched checks the watch budget, not just event
// filtering: on Linux every watched directory consumes an inotify watch.
func TestIgnoredDirectoriesAreNotWatched(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, ".gitignore"), "node_modules/\n")
	write(t, filepath.Join(root, "src", "a.go"), "package a\n")
	for _, d := range []string{"node_modules/x", "node_modules/y", "node_modules/z"} {
		write(t, filepath.Join(root, d, "f.js"), "//\n")
	}

	ic := ignorer.New(root, root, ".astignore", nil)
	w, err := New(Config{Root: root, Ignore: ic})
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = w.Close() }()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	if _, err := w.Start(ctx); err != nil {
		t.Fatal(err)
	}

	for dir := range w.watched {
		if strings.Contains(filepath.ToSlash(dir), "/node_modules") {
			t.Errorf("ignored directory was watched: %s", dir)
		}
	}
	if len(w.watched) == 0 {
		t.Error("no directories watched at all — the test would not detect a regression")
	}
	t.Logf("watched %d directories (node_modules excluded)", len(w.watched))
}

// TestAcceptFilter covers the extension filter used to skip files no parser
// handles.
func TestAcceptFilter(t *testing.T) {
	root := t.TempDir()
	w, err := New(Config{
		Root:        root,
		Debounce:    60 * time.Millisecond,
		MaxDebounce: 500 * time.Millisecond,
		Accept:      func(p string) bool { return strings.HasSuffix(p, ".go") },
	})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer func() { cancel(); _ = w.Close() }()
	ch, err := w.Start(ctx)
	if err != nil {
		t.Fatal(err)
	}

	write(t, filepath.Join(root, "skip.txt"), "x\n")
	write(t, filepath.Join(root, "take.go"), "package t\n")

	b, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool { return contains(b.Changed, "take.go") })
	if !ok {
		t.Fatal("did not observe take.go")
	}
	if contains(b.Changed, "skip.txt") {
		t.Error("Accept filter did not suppress skip.txt")
	}
}

// TestNewDirectoryIsWatched covers the mkdir race: a directory created after
// startup must be watched, and files already inside it must still be reported.
func TestNewDirectoryIsWatched(t *testing.T) {
	root := t.TempDir()
	ch := startWatcher(t, root, nil)

	sub := filepath.Join(root, "added", "deep")
	write(t, filepath.Join(sub, "new.go"), "package n\n")

	if _, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Changed, "added/deep/new.go")
	}); !ok {
		t.Fatal("file created in a brand-new directory was not reported")
	}

	// A later write in that directory must also be seen, proving the watch stuck.
	write(t, filepath.Join(sub, "second.go"), "package s\n")
	if _, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Changed, "added/deep/second.go")
	}); !ok {
		t.Fatal("subsequent write in the new directory was not reported")
	}
}

// TestDebounceCoalesces verifies a burst becomes one batch rather than many.
func TestDebounceCoalesces(t *testing.T) {
	root := t.TempDir()
	ch := startWatcher(t, root, nil)

	for i := 0; i < 20; i++ {
		write(t, filepath.Join(root, "f"+string(rune('a'+i))+".go"), "package x\n")
	}
	b, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool { return len(b.Changed) > 1 })
	if !ok {
		t.Fatal("expected a coalesced batch with several files")
	}
	t.Logf("coalesced %d files into one batch", len(b.Changed))
}

// A subdirectory's own ignore file scopes to that subdirectory while watching:
// `.opencode/.gitignore` with `node_modules` must not deliver events from
// `.opencode/node_modules/`, but events from `.opencode/keep.js` arrive.
func TestSubdirectoryIgnoreFilesApplyWhileWatching(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, ".opencode", ".gitignore"), "node_modules\n")
	write(t, filepath.Join(root, "src", "keep.go"), "package k\n")

	ic := ignorer.New(root, root, ".astignore", nil)
	ch := startWatcher(t, root, ic)

	write(t, filepath.Join(root, ".opencode", "node_modules", "x.js"), "const x = 1;\n")
	write(t, filepath.Join(root, ".opencode", "keep.js"), "const k = 1;\n")

	b, ok := waitBatch(t, ch, 3*time.Second, func(b Batch) bool {
		return contains(b.Changed, ".opencode/keep.js")
	})
	if !ok {
		t.Fatal("did not observe the kept file inside .opencode")
	}
	if contains(b.Changed, ".opencode/node_modules/x.js") || contains(b.Removed, ".opencode/node_modules/x.js") {
		t.Errorf("subdirectory-ignored path leaked into batch: %v (batch=%v)", b.Changed, b.Changed)
	}
}
