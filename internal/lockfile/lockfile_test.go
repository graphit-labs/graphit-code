package lockfile

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

func TestTryAcquireExcludesASecondHolder(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sync.lock")

	first, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("first TryAcquire: %v", err)
	}

	second, err := TryAcquire(path)
	if !errors.Is(err, ErrLocked) {
		second.Release()
		t.Fatalf("second TryAcquire err = %v, want ErrLocked", err)
	}
	if second != nil {
		t.Error("second TryAcquire returned a lock alongside ErrLocked")
	}

	first.Release()

	third, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("TryAcquire after Release: %v", err)
	}
	third.Release()
}

func TestReleaseIsIdempotent(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sync.lock")

	lock, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("TryAcquire: %v", err)
	}
	lock.Release()
	lock.Release()

	var nilLock *Lock
	nilLock.Release()
}

func TestTryAcquireCreatesMissingParentDir(t *testing.T) {
	path := filepath.Join(t.TempDir(), "nested", "dir", "sync.lock")

	lock, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("TryAcquire: %v", err)
	}
	defer lock.Release()

	if _, err := os.Stat(path); err != nil {
		t.Fatalf("lock file not created: %v", err)
	}
	if lock.Path() != path {
		t.Errorf("Path() = %q, want %q", lock.Path(), path)
	}
}

func TestHeldLockNamesItsHolder(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sync.lock")

	lock, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("TryAcquire: %v", err)
	}
	defer lock.Release()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("reading lock file: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) < 2 {
		t.Fatalf("lock file has %d line(s), want pid and timestamp: %q", len(lines), data)
	}
	if pid, err := strconv.Atoi(strings.TrimSpace(lines[0])); err != nil || pid != os.Getpid() {
		t.Errorf("first line = %q, want pid %d", lines[0], os.Getpid())
	}
}

// TestAcquireWaitsForTheHolder covers the case TryAcquire cannot serve: work that still has
// to happen and merely must not happen concurrently.
func TestAcquireWaitsForTheHolder(t *testing.T) {
	path := filepath.Join(t.TempDir(), "wait.lock")

	held, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("TryAcquire: %v", err)
	}

	released := make(chan struct{})
	go func() {
		time.Sleep(150 * time.Millisecond)
		held.Release()
		close(released)
	}()

	start := time.Now()
	lock, err := Acquire(path, 5*time.Second)
	if err != nil {
		t.Fatalf("Acquire waited and still failed: %v", err)
	}
	defer lock.Release()

	<-released
	if waited := time.Since(start); waited < 100*time.Millisecond {
		t.Errorf("Acquire returned after %v — it did not wait for the holder", waited)
	}
}

// TestAcquireGivesUpAtTheDeadline pins that a wait is bounded: a holder that never lets go
// must not hang the caller forever.
func TestAcquireGivesUpAtTheDeadline(t *testing.T) {
	path := filepath.Join(t.TempDir(), "stuck.lock")

	held, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("TryAcquire: %v", err)
	}
	defer held.Release()

	start := time.Now()
	if _, err := Acquire(path, 120*time.Millisecond); !errors.Is(err, ErrLocked) {
		t.Fatalf("Acquire error = %v, want ErrLocked", err)
	}
	if waited := time.Since(start); waited < 100*time.Millisecond {
		t.Errorf("gave up after %v, before the deadline", waited)
	}
}
