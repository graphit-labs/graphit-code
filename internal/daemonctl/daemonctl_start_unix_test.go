//go:build !windows

package daemonctl

import (
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestConcurrentEnsureRunningStartsOneReadyDaemon(t *testing.T) {
	root := t.TempDir()
	pidPath := filepath.Join(root, "daemon", "daemon.pid")
	countPath := filepath.Join(root, "starts")
	releasePath := filepath.Join(root, "release")
	launcherPath := filepath.Join(root, "launcher")
	script := "#!/bin/sh\n" +
		"printf 'started\\n' >> \"$GRAPHIT_DAEMON_TEST_COUNT\"\n" +
		"exec flock \"$GRAPHIT_DAEMON_TEST_PID\" sh -c 'while [ ! -f \"$GRAPHIT_DAEMON_TEST_RELEASE\" ]; do sleep 0.01; done'\n"
	if err := os.WriteFile(launcherPath, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcherPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_COUNT", countPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_PID", pidPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_RELEASE", releasePath)
	t.Cleanup(func() { _ = os.WriteFile(releasePath, []byte("release"), 0o600) })

	start := make(chan struct{})
	results := make(chan struct {
		started bool
		err     error
	}, 2)
	var ready sync.WaitGroup
	ready.Add(2)
	for range 2 {
		go func() {
			ready.Done()
			<-start
			started, err := EnsureRunning()
			results <- struct {
				started bool
				err     error
			}{started: started, err: err}
		}()
	}
	ready.Wait()
	close(start)

	startedCount := 0
	for range 2 {
		select {
		case result := <-results:
			if result.err != nil {
				t.Fatal(result.err)
			}
			if result.started {
				startedCount++
			}
		case <-time.After(2 * time.Second):
			t.Fatal("concurrent startup did not finish")
		}
	}
	if startedCount != 1 {
		t.Fatalf("started results = %d, want 1", startedCount)
	}
	data, err := os.ReadFile(countPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "started\n" {
		t.Fatalf("launcher starts = %q, want one", data)
	}
	if !fileLocked(pidPath) {
		t.Fatal("daemon PID file was not locked when startup returned")
	}
	if err := os.WriteFile(releasePath, []byte("release"), 0o600); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(time.Second)
	for fileLocked(pidPath) && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if fileLocked(pidPath) {
		t.Fatal("temporary daemon did not release its PID lock")
	}
}
