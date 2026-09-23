//go:build linux

package daemonservice

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func testServiceEnv(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(dir, "config"))
	exe := filepath.Join(dir, "Graphit app%", "graphit")
	if err := os.MkdirAll(filepath.Dir(exe), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(exe, []byte("test"), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_LAUNCHER_PATH", exe)
	cron := filepath.Join(dir, "crontab")
	if err := os.WriteFile(cron, []byte("#!/bin/sh\necho 'no crontab for test' >&2\nexit 1\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir)
	return exe
}

func TestLinuxServiceLifecycle(t *testing.T) {
	exe := testServiceEnv(t)
	var mu sync.Mutex
	var calls []string
	old := systemctlRun
	systemctlRun = func(name string, args ...string) error {
		mu.Lock()
		defer mu.Unlock()
		calls = append(calls, name+" "+strings.Join(args, " "))
		if len(args) > 1 && (args[1] == "is-active" || args[1] == "is-enabled") {
			return errors.New("not active")
		}
		return nil
	}
	t.Cleanup(func() { systemctlRun = old })
	if err := EnsureInstalled(); err != nil {
		t.Fatal(err)
	}
	if err := EnsureInstalled(); err != nil {
		t.Fatal(err)
	}
	path, err := unitPath()
	if err != nil {
		t.Fatal(err)
	}
	unit, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(unit), `ExecStart="`+strings.ReplaceAll(exe, "%", "%%")+`" daemon --managed`) {
		t.Fatalf("ExecStart did not escape path: %s", unit)
	}
	if !strings.Contains(string(unit), "Restart=on-failure") || !strings.Contains(string(unit), "WantedBy=default.target") {
		t.Fatalf("missing service supervision/login settings: %s", unit)
	}
	if err := Start(); err != nil {
		t.Fatal(err)
	}
	if err := Stop(); err != nil {
		t.Fatal(err)
	}
	if err := Restart(); err != nil {
		t.Fatal(err)
	}
	if err := Remove(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("unit still exists: %v", err)
	}
	mu.Lock()
	defer mu.Unlock()
	joined := strings.Join(calls, "\n")
	for _, want := range []string{"daemon-reload", "disable " + unitName(), "start " + unitName(), "stop " + unitName(), "restart " + unitName()} {
		if !strings.Contains(joined, want) {
			t.Errorf("missing %q in %s", want, joined)
		}
	}
}

func TestLinuxAutoStartIsSeparateChoice(t *testing.T) {
	testServiceEnv(t)
	old := systemctlRun
	var calls []string
	systemctlRun = func(name string, args ...string) error {
		calls = append(calls, strings.Join(args, " "))
		return nil
	}
	t.Cleanup(func() { systemctlRun = old })
	if err := Install(true); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(strings.Join(calls, "\n"), "enable "+unitName()) {
		t.Fatalf("auto login not enabled: %v", calls)
	}
	before := len(calls)
	if err := EnsureInstalled(); err != nil {
		t.Fatal(err)
	}
	for _, call := range calls[before:] {
		if strings.Contains(call, " enable ") || strings.Contains(call, " disable ") {
			t.Fatalf("EnsureInstalled modified existing login preference: %v", calls[before:])
		}
	}
}

func TestInstallRollsBackUnitWhenManagerRejectsIt(t *testing.T) {
	testServiceEnv(t)
	old := systemctlRun
	systemctlRun = func(_ string, args ...string) error {
		if len(args) > 1 && args[1] == "disable" {
			return errors.New("manager unavailable")
		}
		return nil
	}
	t.Cleanup(func() { systemctlRun = old })
	if err := Install(false); err == nil {
		t.Fatal("expected install failure")
	}
	path, err := unitPath()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("partial unit registration remains: %v", err)
	}
}

func TestInstallRollsBackWhenLegacyCronCannotBeMigrated(t *testing.T) {
	testServiceEnv(t)
	old := systemctlRun
	systemctlRun = func(_ string, _ ...string) error { return nil }
	t.Cleanup(func() { systemctlRun = old })
	cron := filepath.Join(os.Getenv("HOME"), "crontab")
	if err := os.WriteFile(cron, []byte("#!/bin/sh\necho '# GRAPHIT_DAEMON_SCHEDULER'\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := Install(false); err == nil {
		t.Fatal("expected malformed cron migration error")
	}
	path, err := unitPath()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("service unit left after failed cron migration: %v", err)
	}
}

func TestConcurrentEnsureInstalledCreatesOneUnit(t *testing.T) {
	testServiceEnv(t)
	old := systemctlRun
	var mu sync.Mutex
	reloads := 0
	systemctlRun = func(name string, args ...string) error {
		if len(args) > 1 && args[1] == "daemon-reload" {
			mu.Lock()
			reloads++
			mu.Unlock()
		}
		return nil
	}
	t.Cleanup(func() { systemctlRun = old })
	var wg sync.WaitGroup
	errs := make(chan error, 2)
	for range 2 {
		wg.Add(1)
		go func() { defer wg.Done(); errs <- EnsureInstalled() }()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatal(err)
		}
	}
	if reloads != 1 {
		t.Fatalf("expected one registration, got %d", reloads)
	}
}

func TestRemoveLegacyCronKeepsOtherJobs(t *testing.T) {
	testServiceEnv(t)
	dir := os.Getenv("HOME")
	state := filepath.Join(dir, "crontab-state")
	old := "0 5 * * * /usr/bin/backup\n# GRAPHIT_DAEMON_SCHEDULER\n* * * * * /usr/bin/graphit daemon > /dev/null 2>&1\n15 9 * * * /usr/bin/report\n"
	if err := os.WriteFile(state, []byte(old), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("CRONTAB_STATE", state)
	script := "#!/bin/sh\ncase \"$1\" in\n-l) /bin/cat \"$CRONTAB_STATE\" ;;\n-) /bin/cat > \"$CRONTAB_STATE\" ;;\n*) exit 2 ;;\nesac\n"
	if err := os.WriteFile(filepath.Join(dir, "crontab"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := removeLegacyCron(); err != nil {
		t.Fatal(err)
	}
	actual, err := os.ReadFile(state)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(actual), "GRAPHIT_DAEMON_SCHEDULER") || strings.Contains(string(actual), "graphit daemon") {
		t.Fatalf("legacy job remains: %s", actual)
	}
	for _, want := range []string{"/usr/bin/backup", "/usr/bin/report"} {
		if !strings.Contains(string(actual), want) {
			t.Errorf("removed unrelated job %q: %s", want, actual)
		}
	}
}
