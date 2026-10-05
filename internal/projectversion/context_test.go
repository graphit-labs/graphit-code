package projectversion

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestAcquireContextCancelsWaitingWithoutChangingVersion(t *testing.T) {
	dir := t.TempDir()
	release, err := Acquire(dir, "first")
	if err != nil {
		t.Fatal(err)
	}
	defer release()
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Millisecond)
	defer cancel()
	if _, err := AcquireContext(ctx, dir, "second"); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("waiting: %v", err)
	}
	if got := Current(dir).Version; got != "first" {
		t.Fatalf("version changed: %q", got)
	}
	other, err := AcquireContext(ctx, t.TempDir(), "third")
	if other != nil || !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("canceled scope: %v", err)
	}
}

func TestAcquireContextIsolatesProjectsAndReleasesOverride(t *testing.T) {
	a, b := t.TempDir(), t.TempDir()
	ra, err := AcquireContext(context.Background(), a, "branch-a")
	if err != nil {
		t.Fatal(err)
	}
	rb, err := AcquireContext(context.Background(), b, "branch-b")
	if err != nil {
		t.Fatal(err)
	}
	if Current(a).Version != "branch-a" || Current(b).Version != "branch-b" {
		t.Fatal("cross-project override")
	}
	ra()
	rb()
	if Current(a).Version != DefaultName || Current(b).Version != DefaultName {
		t.Fatal("leaked override")
	}
}
