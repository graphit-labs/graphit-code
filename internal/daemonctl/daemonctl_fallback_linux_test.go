//go:build linux

package daemonctl

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

func TestEnsureRunningFallsBackWhenUserManagerUnavailable(t *testing.T) {
	root := t.TempDir()
	testExe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	launcher := filepath.Join(root, "launcher")
	argsPath := filepath.Join(root, "args")
	if err := os.WriteFile(launcher, []byte("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$GRAPHIT_DAEMON_TEST_ARGS\"\nexec \"$GRAPHIT_DAEMON_TEST_HELPER\" -test.run '^TestDaemonctlLauncherHelper$'\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "systemctl"), []byte("#!/bin/sh\nexit 1\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcher)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER", testExe)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER_PROCESS", "1")
	t.Setenv("GRAPHIT_DAEMON_TEST_ARGS", argsPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_COUNT", filepath.Join(root, "starts"))
	t.Setenv("GRAPHIT_DAEMON_TEST_PID", filepath.Join(root, "daemon", "daemon.pid"))
	t.Setenv("GRAPHIT_DAEMON_TEST_RELEASE", filepath.Join(root, "release"))
	t.Setenv("PATH", root)
	t.Cleanup(func() { _ = Stop() })
	if err := daemonservice.Install(false); err == nil {
		t.Fatal("explicit service installation succeeded without a user manager")
	}

	var ready sync.WaitGroup
	ready.Add(2)
	results := make(chan error, 2)
	for range 2 {
		go func() {
			ready.Done()
			_, err := EnsureRunning()
			results <- err
		}()
	}
	ready.Wait()
	for range 2 {
		if err := <-results; err != nil {
			t.Fatal(err)
		}
	}
	args, err := os.ReadFile(argsPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(args) != "daemon\n--ui\n" {
		t.Fatalf("fallback args = %q, want daemon --ui", args)
	}
	starts, err := os.ReadFile(filepath.Join(root, "starts"))
	if err != nil {
		t.Fatal(err)
	}
	if string(starts) != "started\n" {
		t.Fatalf("daemon starts = %q, want one", starts)
	}
	if err := Stop(); err != nil {
		t.Fatal(err)
	}
	if IsRunning() {
		t.Fatal("direct daemon still holds its PID lock after stop")
	}
}

func TestEnsureRunningDoesNotHideServiceInstallationError(t *testing.T) {
	root := t.TempDir()
	launcher := filepath.Join(root, "launcher")
	if err := os.WriteFile(launcher, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	manager := "#!/bin/sh\ncase \"$2\" in\nshow-environment) exit 0 ;;\ndaemon-reload) exit 1 ;;\n*) exit 0 ;;\nesac\n"
	if err := os.WriteFile(filepath.Join(root, "systemctl"), []byte(manager), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcher)
	t.Setenv("PATH", root)
	_, err := EnsureRunning()
	if err == nil || !strings.Contains(err.Error(), "registering daemon service") {
		t.Fatalf("service installation error = %v, want registration failure", err)
	}
	if IsRunning() {
		t.Fatal("daemon started despite an available but failing service manager")
	}
}
