package brand

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestGlobalDirEnvOverrideWinsOverHome(t *testing.T) {
	home := t.TempDir()
	elsewhere := filepath.Join(t.TempDir(), "store")

	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(EnvVar("GLOBAL_DIR"), elsewhere)

	if got := GlobalDir(); got != elsewhere {
		t.Fatalf("GlobalDir() = %q; want the override %q", got, elsewhere)
	}
	if strings.HasPrefix(GlobalDir(), home) {
		t.Fatalf("GlobalDir() = %q is still under HOME %q", GlobalDir(), home)
	}
}

func TestGlobalDirWithoutOverrideStaysUnderHome(t *testing.T) {
	home := t.TempDir()

	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	if err := os.Unsetenv(EnvVar("GLOBAL_DIR")); err != nil {
		t.Fatalf("Unsetenv: %v", err)
	}

	if got, want := GlobalDir(), filepath.Join(home, DotDir()); got != want {
		t.Fatalf("GlobalDir() = %q; want %q", got, want)
	}
}

func TestGlobalDirBlankOverrideIsTreatedAsUnset(t *testing.T) {
	home := t.TempDir()
	want := filepath.Join(home, DotDir())

	for _, blank := range []string{"", "   ", "\t", "\n"} {
		t.Setenv("HOME", home)
		t.Setenv("USERPROFILE", home)
		t.Setenv(EnvVar("GLOBAL_DIR"), blank)

		if got := GlobalDir(); got != want {
			t.Fatalf("GlobalDir() with override %q = %q; want the home default %q", blank, got, want)
		}
	}
}

func TestGlobalDirOverrideFollowsTheBrandName(t *testing.T) {
	original := Brand
	defer func() { Brand = original }()

	branded := filepath.Join(t.TempDir(), "acme-store")
	other := filepath.Join(t.TempDir(), "graphit-store")

	t.Setenv("HOME", t.TempDir())
	t.Setenv("ACME_GLOBAL_DIR", branded)
	t.Setenv("GRAPHIT_GLOBAL_DIR", other)

	Brand = "acme"

	if got := GlobalDir(); got != branded {
		t.Fatalf("GlobalDir() = %q; want %q — the override must be named after the brand in use", got, branded)
	}
}

// A relative override must not move when the process changes directory. The daemon
// chdirs into GlobalDir(), so a value resolved against the live working directory would
// answer one level deeper every time it was asked again.
func TestGlobalDirRelativeOverrideDoesNotDriftOnChdir(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	t.Setenv(EnvVar("GLOBAL_DIR"), "relative-store")

	first := GlobalDir()
	if !filepath.IsAbs(first) {
		t.Fatalf("GlobalDir() = %q; a relative override must resolve to an absolute path", first)
	}
	if want := filepath.Join(processStartDir, "relative-store"); first != want {
		t.Fatalf("GlobalDir() = %q; want %q, resolved against the process start directory", first, want)
	}

	if err := os.MkdirAll(first, 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	t.Chdir(first)

	if second := GlobalDir(); second != first {
		t.Fatalf("GlobalDir() returned %q after chdir and %q before it — the override drifted", second, first)
	}
}

func TestGlobalDirOverrideCarriesEveryDerivedPath(t *testing.T) {
	elsewhere := filepath.Join(t.TempDir(), "store")

	t.Setenv("HOME", t.TempDir())
	t.Setenv(EnvVar("GLOBAL_DIR"), elsewhere)

	derived := map[string]string{
		"GlobalRulesDir": GlobalRulesDir(),
		"HubRulesDir":    HubRulesDir(),
		"RuntimeDir":     RuntimeDir("1.2.3"),
	}
	for name, got := range derived {
		if !strings.HasPrefix(got, elsewhere+string(os.PathSeparator)) {
			t.Errorf("%s() = %q; want a path under the overridden global dir %q", name, got, elsewhere)
		}
	}
}
