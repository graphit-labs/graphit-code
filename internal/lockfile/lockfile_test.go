package lockfile

import (
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestLockHeldByExitedProcessIsRecoverable(t *testing.T) {
	path := filepath.Join(t.TempDir(), "crash.lock")
	ready := path + ".ready"
	cmd := exec.Command(os.Args[0], "-test.run=^TestLockfileChild$")
	cmd.Env = append(os.Environ(), "GRAPHIT_TEST_LOCKFILE_CHILD="+path, "GRAPHIT_TEST_LOCKFILE_READY="+ready)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { _ = cmd.Process.Kill(); _ = cmd.Wait() }()
	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := os.Stat(ready); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("child never acquired lock")
		}
		time.Sleep(10 * time.Millisecond)
	}
	if lock, err := TryAcquire(path); !errors.Is(err, ErrLocked) {
		lock.Release()
		t.Fatalf("lock held by child = %v, want ErrLocked", err)
	}
	if err := stdin.Close(); err != nil {
		t.Fatal(err)
	}
	if err := cmd.Wait(); err != nil {
		t.Fatal(err)
	}
	lock, err := TryAcquire(path)
	if err != nil {
		t.Fatalf("lock after child exit: %v", err)
	}
	lock.Release()
}

func TestLockfileChild(t *testing.T) {
	path := os.Getenv("GRAPHIT_TEST_LOCKFILE_CHILD")
	if path == "" {
		return
	}
	lock, err := TryAcquire(path)
	if err != nil {
		t.Fatal(err)
	}
	_ = lock // Deliberately exit without Release to exercise OS cleanup.
	if err := os.WriteFile(os.Getenv("GRAPHIT_TEST_LOCKFILE_READY"), []byte("ready"), 0o600); err != nil {
		t.Fatal(err)
	}
	var b [1]byte
	_, _ = os.Stdin.Read(b[:])
	os.Exit(0)
}

func TestNonContentionFlockErrorIsDistinct(t *testing.T) {
	f, err := os.CreateTemp(t.TempDir(), "closed-lock")
	if err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}
	err = flockTry(f)
	if err == nil || flockContended(err) {
		t.Fatalf("flock on closed file = %v, want non-contention error", err)
	}
}

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

func TestConcurrentReleaseIsIdempotent(t *testing.T) {
	lock, err := TryAcquire(filepath.Join(t.TempDir(), "concurrent.lock"))
	if err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); lock.Release() }()
	}
	wg.Wait()
	other, err := TryAcquire(lock.Path())
	if err != nil {
		t.Fatalf("reacquire after concurrent release: %v", err)
	}
	other.Release()
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

	if _, err := lock.f.Seek(0, 0); err != nil {
		t.Fatalf("seeking lock stamp: %v", err)
	}
	// Windows denies reads through a second handle while byte zero is locked.
	data, err := io.ReadAll(lock.f)
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
