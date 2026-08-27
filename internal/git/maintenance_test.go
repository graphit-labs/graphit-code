package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// The whole fix rests on git actually reading this, so the test asks GIT itself rather
// than asserting on the variables we just set. Anything less would be checking that we
// can call os.Setenv.
func TestDisableAutoMaintenanceIsSeenByGit(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not available")
	}

	// TestMain does not run for this package's own test, so set it here — and through
	// t.Setenv so it is undone afterwards.
	t.Setenv("GIT_CONFIG_COUNT", "")
	os.Unsetenv("GIT_CONFIG_COUNT")
	DisableAutoMaintenance()
	t.Cleanup(func() {
		for _, k := range []string{"GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0",
			"GIT_CONFIG_KEY_1", "GIT_CONFIG_VALUE_1"} {
			os.Unsetenv(k)
		}
	})

	repo := filepath.Join(t.TempDir(), "repo")
	if err := os.MkdirAll(repo, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	run := func(args ...string) string {
		cmd := exec.Command("git", append([]string{"-C", repo}, args...)...)
		cmd.Env = os.Environ()
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
		}
		return strings.TrimSpace(string(out))
	}
	run("init")

	if got := run("config", "--get", "gc.auto"); got != "0" {
		t.Errorf("git reports gc.auto = %q, want 0 — the setting never reached it", got)
	}
	if got := run("config", "--get", "maintenance.auto"); got != "false" {
		t.Errorf("git reports maintenance.auto = %q, want false", got)
	}
}

// Appending, not replacing: exported configuration that was already there has to
// survive, or this quietly drops someone else's setting.
func TestDisableAutoMaintenanceKeepsConfigurationAlreadyExported(t *testing.T) {
	t.Setenv("GIT_CONFIG_COUNT", "1")
	t.Setenv("GIT_CONFIG_KEY_0", "user.name")
	t.Setenv("GIT_CONFIG_VALUE_0", "Someone Else")

	DisableAutoMaintenance()
	t.Cleanup(func() {
		for _, k := range []string{"GIT_CONFIG_KEY_1", "GIT_CONFIG_VALUE_1",
			"GIT_CONFIG_KEY_2", "GIT_CONFIG_VALUE_2"} {
			os.Unsetenv(k)
		}
	})

	if got := os.Getenv("GIT_CONFIG_KEY_0"); got != "user.name" {
		t.Errorf("pre-existing key was overwritten: GIT_CONFIG_KEY_0 = %q", got)
	}
	if got := os.Getenv("GIT_CONFIG_COUNT"); got != "3" {
		t.Errorf("GIT_CONFIG_COUNT = %q, want 3 (one existing plus two added)", got)
	}
	if got := os.Getenv("GIT_CONFIG_KEY_1"); got != "gc.auto" {
		t.Errorf("the new keys did not land after the existing one: KEY_1 = %q", got)
	}
}
