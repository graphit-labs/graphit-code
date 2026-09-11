package storelifecycle

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestLockSurvivesTargetRemovalAndContextIsReentrant(t *testing.T) {
	targetDir := filepath.Join(t.TempDir(), "store")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatal(err)
	}

	ctx, held, err := TryAcquire(context.Background(), targetDir)
	if err != nil {
		t.Fatalf("acquire lifecycle lock: %v", err)
	}
	defer held.Release()

	if err := os.RemoveAll(targetDir); err != nil {
		t.Fatal(err)
	}
	if _, _, err := TryAcquire(context.Background(), targetDir); !errors.Is(err, ErrLocked) {
		t.Fatalf("independent acquisition after removing target = %v, want ErrLocked", err)
	}

	_, nested, err := TryAcquire(ctx, targetDir)
	if err != nil {
		t.Fatalf("nested acquisition through owning context: %v", err)
	}
	nested.Release()

	held.Release()
	_, reacquired, err := TryAcquire(context.Background(), targetDir)
	if err != nil {
		t.Fatalf("reacquire lifecycle lock: %v", err)
	}
	reacquired.Release()
}

func TestTryAcquireHonorsCanceledContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, _, err := TryAcquire(ctx, filepath.Join(t.TempDir(), "store")); !errors.Is(err, context.Canceled) {
		t.Fatalf("TryAcquire with canceled context = %v, want context.Canceled", err)
	}
}
