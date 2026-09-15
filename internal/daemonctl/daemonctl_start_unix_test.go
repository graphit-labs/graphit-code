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
		"exec \"$GRAPHIT_DAEMON_TEST_HELPER\" -test.run '^TestDaemonctlLauncherHelper$'\n"
	if err := os.WriteFile(launcherPath, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	testExe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcherPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER", testExe)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER_PROCESS", "1")
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

func TestDaemonctlLauncherHelper(t *testing.T) {
	if os.Getenv("GRAPHIT_DAEMON_TEST_HELPER_PROCESS") != "1" {
		return
	}

	countFile, err := os.OpenFile(os.Getenv("GRAPHIT_DAEMON_TEST_COUNT"), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := countFile.WriteString("started\n"); err != nil {
		_ = countFile.Close()
		t.Fatal(err)
	}
	if err := countFile.Close(); err != nil {
		t.Fatal(err)
	}

	pidFile, err := os.OpenFile(os.Getenv("GRAPHIT_DAEMON_TEST_PID"), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	defer pidFile.Close()
	if err := flockExclusiveBlocking(pidFile); err != nil {
		t.Fatal(err)
	}
	defer flockProbeRelease(pidFile)

	for {
		if _, err := os.Stat(os.Getenv("GRAPHIT_DAEMON_TEST_RELEASE")); err == nil {
			return
		} else if !os.IsNotExist(err) {
			t.Fatal(err)
		}
		time.Sleep(10 * time.Millisecond)
	}
}
