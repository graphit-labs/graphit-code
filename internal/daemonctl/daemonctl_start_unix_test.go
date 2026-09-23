//go:build linux || darwin

package daemonctl

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
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
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcherPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER", testExe)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER_PROCESS", "1")
	t.Setenv("GRAPHIT_DAEMON_TEST_COUNT", countPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_PID", pidPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_RELEASE", releasePath)
	managerState := filepath.Join(root, "manager-active")
	t.Setenv("GRAPHIT_DAEMON_TEST_MANAGER", managerState)
	installFakeServiceManager(t, root, runtime.GOOS)
	t.Setenv("PATH", root)
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
		status, statusErr := daemonservice.GetStatus()
		locked, lockErr := fileLockState(pidPath)
		starts, startsErr := os.ReadFile(countPath)
		t.Fatalf("started results = %d, want 1 (service=%+v serviceErr=%v pidLocked=%t lockErr=%v starts=%q startsErr=%v)",
			startedCount, status, statusErr, locked, lockErr, starts, startsErr)
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
	if err := Stop(); err != nil {
		t.Fatal(err)
	}
	if fileLocked(pidPath) {
		t.Fatal("daemon remained alive after intentional service stop")
	}
	managerData, err := os.ReadFile(managerState)
	if err != nil || string(managerData) != "stopped\n" {
		t.Fatalf("manager state after stop = %q, err=%v; want stopped", managerData, err)
	}
	if started, err := EnsureRunning(); err != nil || !started {
		t.Fatalf("restart after stop started=%t err=%v", started, err)
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

func TestEnsureRunningAdoptsLegacyDaemonBeforeManagedStart(t *testing.T) {
	root := t.TempDir()
	pidPath := filepath.Join(root, "daemon", "daemon.pid")
	countPath := filepath.Join(root, "starts")
	releasePath := filepath.Join(root, "release")
	launcherPath := filepath.Join(root, "launcher")
	testExe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(launcherPath, []byte("#!/bin/sh\nexec \"$GRAPHIT_DAEMON_TEST_HELPER\" -test.run '^TestDaemonctlLauncherHelper$'\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), root)
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcherPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER", testExe)
	t.Setenv("GRAPHIT_DAEMON_TEST_HELPER_PROCESS", "1")
	t.Setenv("GRAPHIT_DAEMON_TEST_COUNT", countPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_PID", pidPath)
	t.Setenv("GRAPHIT_DAEMON_TEST_RELEASE", releasePath)
	t.Setenv("GRAPHIT_DAEMON_TEST_MANAGER", filepath.Join(root, "manager-active"))
	installFakeServiceManager(t, root, runtime.GOOS)
	t.Setenv("PATH", root)
	t.Cleanup(func() { _ = os.WriteFile(releasePath, []byte("release"), 0o600) })
	if err := os.MkdirAll(filepath.Dir(pidPath), 0o700); err != nil {
		t.Fatal(err)
	}
	legacy := exec.Command(launcherPath, "daemon")
	if err := legacy.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { _ = legacy.Process.Kill(); _ = legacy.Wait() }()
	if err := waitForFileLock(pidPath, time.Second, time.Millisecond); err != nil {
		t.Fatal(err)
	}
	started, err := EnsureRunning()
	if err != nil || !started {
		t.Fatalf("adoption started=%t err=%v", started, err)
	}
	data, err := os.ReadFile(pidPath)
	if err != nil {
		t.Fatal(err)
	}
	managedPID, err := strconv.Atoi(strings.TrimSpace(strings.SplitN(string(data), "\n", 2)[0]))
	if err != nil || managedPID == legacy.Process.Pid {
		t.Fatalf("managed PID=%d legacy PID=%d err=%v", managedPID, legacy.Process.Pid, err)
	}
	starts, err := os.ReadFile(countPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(starts) != "started\nstarted\n" {
		t.Fatalf("expected one legacy and one managed start, got %q", starts)
	}
}

func TestFakeLaunchctlReportsManagedState(t *testing.T) {
	root := t.TempDir()
	managerState := filepath.Join(root, "manager-active")
	launcherPath := filepath.Join(root, "launcher")
	if err := os.WriteFile(launcherPath, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_DAEMON_TEST_MANAGER", managerState)
	t.Setenv(brand.EnvVar("LAUNCHER_PATH"), launcherPath)
	installFakeServiceManager(t, root, "darwin")
	t.Setenv("PATH", root)
	manager := filepath.Join(root, "launchctl")
	if err := exec.Command(manager, "print", "gui/test/graphit-daemon").Run(); err == nil {
		t.Fatal("unstarted fake LaunchAgent is active")
	}
	if err := exec.Command(manager, "bootstrap", "gui/test", "graphit-daemon.plist").Run(); err != nil {
		t.Fatal(err)
	}
	if err := exec.Command(manager, "print", "gui/test/graphit-daemon").Run(); err != nil {
		t.Fatalf("started fake LaunchAgent is inactive: %v", err)
	}
	if err := exec.Command(manager, "bootout", "gui/test/graphit-daemon").Run(); err != nil {
		t.Fatal(err)
	}
	if err := exec.Command(manager, "print", "gui/test/graphit-daemon").Run(); err == nil {
		t.Fatal("stopped fake LaunchAgent is active")
	}
}

func installFakeServiceManager(t *testing.T, root, goos string) {
	t.Helper()
	var name, manager string
	switch goos {
	case "linux":
		name = "systemctl"
		manager = "#!/bin/sh\ncase \"$2\" in\n" +
			"is-active) IFS= read -r state < \"$GRAPHIT_DAEMON_TEST_MANAGER\" && [ \"$state\" = active ] ;;\n" +
			"is-enabled) exit 1 ;;\n" +
			"start) printf 'active\\n' > \"$GRAPHIT_DAEMON_TEST_MANAGER\" || exit 1; \"$GRAPHIT_LAUNCHER_PATH\" daemon --managed >/dev/null 2>&1 & ;;\n" +
			"stop) printf 'stopped\\n' > \"$GRAPHIT_DAEMON_TEST_MANAGER\" || exit 1 ;;\n" +
			"*) exit 0 ;;\n" +
			"esac\n"
		if err := os.WriteFile(filepath.Join(root, "crontab"), []byte("#!/bin/sh\necho 'no crontab for test' >&2\nexit 1\n"), 0o755); err != nil {
			t.Fatal(err)
		}
	case "darwin":
		name = "launchctl"
		manager = "#!/bin/sh\ncase \"$1\" in\n" +
			"print) IFS= read -r state < \"$GRAPHIT_DAEMON_TEST_MANAGER\" && [ \"$state\" = active ] ;;\n" +
			"bootstrap) printf 'active\\n' > \"$GRAPHIT_DAEMON_TEST_MANAGER\" || exit 1; \"$GRAPHIT_LAUNCHER_PATH\" daemon --managed >/dev/null 2>&1 & ;;\n" +
			"bootout) printf 'stopped\\n' > \"$GRAPHIT_DAEMON_TEST_MANAGER\" || exit 1 ;;\n" +
			"*) exit 1 ;;\n" +
			"esac\n"
	}
	if err := os.WriteFile(filepath.Join(root, name), []byte(manager), 0o755); err != nil {
		t.Fatal(err)
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
	if err := pidFile.Truncate(0); err != nil {
		t.Fatal(err)
	}
	if _, err := pidFile.WriteString(strconv.Itoa(os.Getpid()) + "\n"); err != nil {
		t.Fatal(err)
	}

	for {
		if _, err := os.Stat(os.Getenv("GRAPHIT_DAEMON_TEST_RELEASE")); err == nil {
			return
		} else if !os.IsNotExist(err) {
			t.Fatal(err)
		}
		time.Sleep(10 * time.Millisecond)
	}
}
