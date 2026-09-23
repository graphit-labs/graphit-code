package daemonctl

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

func TestPublishedUIURLBelongsToCurrentDaemonLock(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	lock, err := lockfile.TryAcquire(PIDFilePath())
	if err != nil {
		t.Fatal(err)
	}
	defer lock.Release()
	clear, err := PublishUIURL("http://127.0.0.1:8091/")
	if err != nil {
		t.Fatal(err)
	}
	if got := PublishedUIURL(); got != "http://127.0.0.1:8091/" {
		t.Fatalf("published URL = %q", got)
	}
	lock.Release()
	if got := PublishedUIURL(); got != "" {
		t.Fatalf("stale URL without daemon lock = %q", got)
	}
	clear()
	if got := PublishedUIURL(); got != "" {
		t.Fatalf("URL persisted after cleanup: %q", got)
	}
}
