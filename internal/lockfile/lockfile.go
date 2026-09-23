// Package lockfile provides advisory, cross-process file locks — the primitive that
// keeps two copies of the same background job from running over the same project at
// once.
//
// The lock lives in the open-file description, not in the file's contents, so the
// kernel releases it when the holder exits however it exits. There is no stale lock to
// clean up and no PID to validate before trusting it. The pid and timestamp written
// into the file are for whoever opens it while debugging; nothing decides anything
// from them.
package lockfile

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// ErrLocked reports that another process holds the lock. It is the expected outcome,
// not a failure: callers skip their work and return.
var ErrLocked = errors.New("lockfile: held by another process")

// DaemonPIDLockOffset is outside the text stamp on Windows, where a byte-range
// lock lets status readers inspect the PID while the daemon holds the lock.
const DaemonPIDLockOffset = 1024

// Lock is an acquired advisory lock. The zero value is not usable; get one from
// TryAcquire.
type Lock struct {
	path string
	f    *os.File
	once sync.Once
}

// TryAcquire takes an exclusive lock on path without blocking, creating the file and
// its parent directory as needed. It returns ErrLocked when someone else holds it.
func TryAcquire(path string) (*Lock, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, fmt.Errorf("lockfile: create dir: %w", err)
	}

	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, fmt.Errorf("lockfile: open %s: %w", path, err)
	}

	if err := flockTry(f); err != nil {
		_ = f.Close()
		if flockContended(err) {
			return nil, ErrLocked
		}
		return nil, fmt.Errorf("lockfile: lock %s: %w", path, err)
	}

	// Best effort: an unwritable stamp costs a debugging hint, never the lock.
	if err := f.Truncate(0); err == nil {
		if _, err := f.Seek(0, 0); err == nil {
			_, _ = fmt.Fprintf(f, "%d\n%s\n", os.Getpid(), time.Now().UTC().Format(time.RFC3339))
		}
	}

	return &Lock{path: path, f: f}, nil
}

// IsLocked probes an existing lock without changing its file contents.
func IsLocked(path string) (bool, error) {
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if os.IsNotExist(err) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	defer f.Close()
	if err := flockTry(f); err != nil {
		if flockContended(err) {
			return true, nil
		}
		return false, err
	}
	flockRelease(f)
	return false, nil
}

// Acquire takes an exclusive lock on path, waiting up to wait for whoever holds it to
// let go. It returns ErrLocked when the wait elapses first.
//
// TryAcquire is right when the work is redundant — a second copy of a background job has
// nothing to add, so it returns. Acquire is for work that still has to happen and merely
// must not happen at the same time as someone else's: two processes pulling the same git
// repository both need their pull, one after the other.
func Acquire(path string, wait time.Duration) (*Lock, error) {
	deadline := time.Now().Add(wait)
	for {
		lock, err := TryAcquire(path)
		if !errors.Is(err, ErrLocked) {
			return lock, err
		}
		if !time.Now().Before(deadline) {
			return nil, ErrLocked
		}
		time.Sleep(pollInterval)
	}
}

const pollInterval = 50 * time.Millisecond

// Release drops the lock. Calling it more than once, or on a nil Lock, is a no-op.
func (l *Lock) Release() {
	if l == nil {
		return
	}
	l.once.Do(func() {
		if l.f == nil {
			return
		}
		_ = l.f.Truncate(0)
		flockRelease(l.f)
		_ = l.f.Close()
	})
}

// Path returns the file backing the lock.
func (l *Lock) Path() string {
	if l == nil {
		return ""
	}
	return l.path
}
