package ast

import (
	"context"
	"errors"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

func TestIncrementalPipelineWaitsForStoreLifecycle(t *testing.T) {
	storeDir := filepath.Join(t.TempDir(), "store")
	_, held, err := storelifecycle.TryAcquire(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("acquire lifecycle lock: %v", err)
	}
	defer held.Release()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err = RunPipelineForPaths(ctx, nil, t.TempDir(), []string{"changed.go"}, nil, PipelineOptions{
		CacheDir: storeDir,
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("incremental pipeline while lifecycle locked = %v, want context canceled", err)
	}
}

func TestWatcherLocksPipelineAndClosesBackendBeforeRelease(t *testing.T) {
	storeDir := filepath.Join(t.TempDir(), "store")
	_, held, err := storelifecycle.TryAcquire(context.Background(), storeDir)
	if err != nil {
		t.Fatalf("acquire lifecycle lock: %v", err)
	}

	db := &watcherLifecycleDB{storeDir: storeDir}
	watcher := &Watcher{db: db, cfg: WatcherConfig{StoreDir: storeDir}}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	called := false
	watcher.runPipeline(ctx, func(context.Context) { called = true })
	if called || db.closeCount != 0 {
		t.Fatalf("blocked watcher ran=%v closes=%d, want false/0", called, db.closeCount)
	}

	held.Release()
	watcher.runPipeline(context.Background(), func(lockedCtx context.Context) {
		called = true
		if _, nested, err := storelifecycle.TryAcquire(lockedCtx, storeDir); err != nil {
			t.Fatalf("nested pipeline acquisition: %v", err)
		} else {
			nested.Release()
		}
	})
	if !called || db.closeCount != 1 || !db.closeSawLocked {
		t.Fatalf("watcher ran=%v closes=%d close_saw_lock=%v, want true/1/true", called, db.closeCount, db.closeSawLocked)
	}
	if _, reacquired, err := storelifecycle.TryAcquire(context.Background(), storeDir); err != nil {
		t.Fatalf("lock remained held after watcher cycle: %v", err)
	} else {
		reacquired.Release()
	}
}

type watcherLifecycleDB struct {
	storeDir       string
	closeCount     int
	closeSawLocked bool
}

func (*watcherLifecycleDB) Query(context.Context, string, map[string]any) (*QueryResult, error) {
	return &QueryResult{}, nil
}

func (*watcherLifecycleDB) Execute(context.Context, string, map[string]any) (*QueryResult, error) {
	return &QueryResult{}, nil
}

func (*watcherLifecycleDB) ExecuteBatch(context.Context, []BatchQuery) error { return nil }
func (*watcherLifecycleDB) Ping(context.Context) error                       { return nil }
func (*watcherLifecycleDB) BackendType() string                              { return "test" }
func (db *watcherLifecycleDB) Close() error {
	db.closeCount++
	_, guard, err := storelifecycle.TryAcquire(context.Background(), db.storeDir)
	db.closeSawLocked = errors.Is(err, storelifecycle.ErrLocked)
	if guard != nil {
		guard.Release()
	}
	return nil
}
