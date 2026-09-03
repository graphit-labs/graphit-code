package commands

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestAcquireSyncLockRefusesASecondHolder(t *testing.T) {
	wd := t.TempDir()

	first, proceed := acquireSyncLock(wd, "sync.lock")
	if !proceed {
		t.Fatal("first acquireSyncLock refused to proceed on a free lock")
	}

	if second, proceed := acquireSyncLock(wd, "sync.lock"); proceed {
		second.Release()
		t.Error("second acquireSyncLock proceeded while the lock was held")
	}

	if heavy, proceed := acquireSyncLock(wd, "sync-heavy.lock"); !proceed {
		t.Error("the heavy lock is contended by the phase 1 lock")
	} else {
		heavy.Release()
	}

	first.Release()

	third, proceed := acquireSyncLock(wd, "sync.lock")
	if !proceed {
		t.Fatal("acquireSyncLock refused to proceed after Release")
	}
	third.Release()
}

func TestSyncedWithinOnlySkipsWhatItCanProve(t *testing.T) {
	wd := t.TempDir()

	if syncedWithin(wd, time.Minute) {
		t.Error("syncedWithin skipped a sync with no stamp on disk")
	}

	stampSync(wd)

	if !syncedWithin(wd, time.Minute) {
		t.Error("syncedWithin ran a sync that had just finished")
	}
	if syncedWithin(wd, 0) {
		t.Error("a zero window must disable the debounce, not skip everything")
	}
	if syncedWithin(wd, -time.Minute) {
		t.Error("a negative window must disable the debounce")
	}
	if syncedWithin(wd, time.Nanosecond) {
		t.Error("syncedWithin skipped a sync older than the window")
	}
}

func TestSyncedWithinIgnoresAnUnreadableStamp(t *testing.T) {
	wd := t.TempDir()
	path := filepath.Join(wd, brand.DotDir(), "sync.stamp")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(path, []byte("not a timestamp\n"), 0o644); err != nil {
		t.Fatalf("write stamp: %v", err)
	}

	if syncedWithin(wd, time.Hour) {
		t.Error("a garbage stamp must read as 'no idea' and run the sync, not skip it")
	}
}

func TestStampSyncCreatesTheRuntimeDir(t *testing.T) {
	wd := t.TempDir()
	stampSync(wd)

	data, err := os.ReadFile(brand.ProjectRuntimePath(wd, "sync.stamp"))
	if err != nil {
		t.Fatalf("stamp not written: %v", err)
	}
	if _, err := time.Parse(time.RFC3339, string(data[:len(data)-1])); err != nil {
		t.Errorf("stamp is not RFC3339: %q (%v)", data, err)
	}
}
