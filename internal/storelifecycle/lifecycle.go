// Package storelifecycle coordinates long-lived readers and background writers with
// pipelines that publish or destructively replace a local store directory.
package storelifecycle

import (
	"context"
	"errors"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

const pollInterval = 50 * time.Millisecond

// ErrLocked reports that another process owns the store lifecycle.
var ErrLocked = lockfile.ErrLocked

type heldPathsKey struct{}

// Guard owns a lifecycle lock. Release is safe on a nil or no-op guard.
type Guard struct {
	lock *lockfile.Lock
}

// Release gives up the lifecycle lock.
func (g *Guard) Release() {
	if g != nil && g.lock != nil {
		g.lock.Release()
		g.lock = nil
	}
}

// LockPath returns a lock beside the target instead of inside it. A reset can remove
// the complete target directory; placing the lock there would unlink it while a holder
// still owns the old inode and let another process lock a newly created file.
func LockPath(targetDir string) string {
	clean := filepath.Clean(targetDir)
	if abs, err := filepath.Abs(clean); err == nil {
		clean = abs
	}
	return filepath.Join(filepath.Dir(clean), "."+filepath.Base(clean)+".lifecycle.lock")
}

// TryAcquire takes the lifecycle lock without blocking. The returned context records
// ownership, so a lower-level pipeline receiving it can acquire the same target without
// deadlocking on a lock deliberately taken before a destructive reset.
func TryAcquire(ctx context.Context, targetDir string) (context.Context, *Guard, error) {
	if err := ctx.Err(); err != nil {
		return ctx, nil, err
	}
	if targetDir == "" || contextOwns(ctx, targetDir) {
		return ctx, &Guard{}, nil
	}
	lock, err := lockfile.TryAcquire(LockPath(targetDir))
	if err != nil {
		return ctx, nil, err
	}
	return contextWithTarget(ctx, targetDir), &Guard{lock: lock}, nil
}

// Acquire waits for the lifecycle lock while honoring context cancellation.
func Acquire(ctx context.Context, targetDir string) (context.Context, *Guard, error) {
	for {
		lockedCtx, guard, err := TryAcquire(ctx, targetDir)
		if !errors.Is(err, ErrLocked) {
			return lockedCtx, guard, err
		}

		timer := time.NewTimer(pollInterval)
		select {
		case <-ctx.Done():
			if !timer.Stop() {
				<-timer.C
			}
			return ctx, nil, ctx.Err()
		case <-timer.C:
		}
	}
}

func contextOwns(ctx context.Context, targetDir string) bool {
	held, _ := ctx.Value(heldPathsKey{}).(map[string]struct{})
	_, ok := held[LockPath(targetDir)]
	return ok
}

func contextWithTarget(ctx context.Context, targetDir string) context.Context {
	current, _ := ctx.Value(heldPathsKey{}).(map[string]struct{})
	next := make(map[string]struct{}, len(current)+1)
	for path := range current {
		next[path] = struct{}{}
	}
	next[LockPath(targetDir)] = struct{}{}
	return context.WithValue(ctx, heldPathsKey{}, next)
}
