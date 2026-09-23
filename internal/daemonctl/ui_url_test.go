package daemonctl

import (
	"os"
	"strconv"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestPublishedUIURLBelongsToCurrentDaemonLock(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if err := os.MkdirAll(DaemonDir(), 0o700); err != nil {
		t.Fatal(err)
	}
	pidFile, err := os.OpenFile(PIDFilePath(), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	defer pidFile.Close()
	// Match the daemon's PID lock: on Windows it protects byte 1024 so the
	// PID stamp at the start of the file remains readable by status callers.
	if err := flockExclusiveBlocking(pidFile); err != nil {
		t.Fatal(err)
	}
	locked := true
	defer func() {
		if locked {
			flockProbeRelease(pidFile)
		}
	}()
	if _, err := pidFile.WriteString(strconv.Itoa(os.Getpid()) + "\n"); err != nil {
		t.Fatal(err)
	}
	clear, err := PublishUIURL("http://127.0.0.1:8091/")
	if err != nil {
		t.Fatal(err)
	}
	if got := PublishedUIURL(); got != "http://127.0.0.1:8091/" {
		t.Fatalf("published URL = %q", got)
	}
	flockProbeRelease(pidFile)
	locked = false
	if got := PublishedUIURL(); got != "" {
		t.Fatalf("stale URL without daemon lock = %q", got)
	}
	clear()
	if got := PublishedUIURL(); got != "" {
		t.Fatalf("URL persisted after cleanup: %q", got)
	}
}
