package commands

import (
	"encoding/hex"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
)

func TestDaemonHelpDocumentsWatchConfiguration(t *testing.T) {
	text := newDaemonCmd().Long
	for _, expected := range []string{
		"modules.sync false",
		"GRAPHIT_MODULES_SYNC=false",
		"removes the per-project watcher",
		"explicit graphit sync",
		"mcp.api_key",
		"GRAPHIT_MCP_API_KEY",
		"stable across starts",
	} {
		if !strings.Contains(text, expected) {
			t.Errorf("daemon help does not contain %q", expected)
		}
	}
}

func TestBuildDaemonProjectModulesHonorsSyncSwitch(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	tests := []struct {
		name     string
		value    string
		wantSync bool
	}{
		{name: "default", wantSync: true},
		{name: "disabled", value: "false", wantSync: false},
		{name: "explicitly enabled", value: "true", wantSync: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			projectDir := t.TempDir()
			if tt.value != "" {
				lockfile := `{"config":{"modules":{"sync":"` + tt.value + `"}}}`
				if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lockfile), 0o600); err != nil {
					t.Fatal(err)
				}
			}

			cfg := daemon.DefaultConfig()
			cfg.DisableEmbedding = true
			cfg.DisableDream = true
			modules, _, err := buildDaemonProjectModules(projectDir, cfg, nil)
			if err != nil {
				t.Fatal(err)
			}

			gotSync := false
			for _, module := range modules {
				if module.Name() == "sync" {
					gotSync = true
				}
			}
			if gotSync != tt.wantSync {
				t.Fatalf("sync module present=%v, want %v", gotSync, tt.wantSync)
			}
		})
	}
}

func TestBuildDaemonProjectModulesHonorsTaskSwitch(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	for _, tt := range []struct {
		name     string
		disabled bool
	}{
		{name: "enabled"},
		{name: "disabled", disabled: true},
	} {
		t.Run(tt.name, func(t *testing.T) {
			projectDir := t.TempDir()
			value := "true"
			if tt.disabled {
				value = "false"
			}
			lockfile := `{"project":{"id":"project-id"},"config":{"modules":{"task":"` + value + `"}}}`
			if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lockfile), 0o600); err != nil {
				t.Fatal(err)
			}
			cfg := daemon.DefaultConfig()
			cfg.DisableEmbedding = true
			cfg.DisableDream = true
			modules, _, err := buildDaemonProjectModules(projectDir, cfg, nil)
			if err != nil {
				t.Fatal(err)
			}
			found := false
			for _, module := range modules {
				found = found || module.Name() == "task_maintenance"
			}
			if found == tt.disabled {
				t.Fatalf("task maintenance present=%v, disabled=%v", found, tt.disabled)
			}
		})
	}
}

func TestResolveDaemonMCPAPIKey(t *testing.T) {
	t.Run("configured key", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
		if err := config.SetGlobalConfigValue(config.MCPAPIKeyConfigKey, "stable-daemon-key"); err != nil {
			t.Fatal(err)
		}

		got, err := resolveDaemonMCPAPIKey(nil, nil)
		if err != nil {
			t.Fatal(err)
		}
		if got != "stable-daemon-key" {
			t.Fatalf("resolveDaemonMCPAPIKey = %q, want configured value", got)
		}
	})

	t.Run("generated fallback", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

		first, err := resolveDaemonMCPAPIKey(nil, nil)
		if err != nil {
			t.Fatal(err)
		}
		second, err := resolveDaemonMCPAPIKey(nil, nil)
		if err != nil {
			t.Fatal(err)
		}
		if first == second {
			t.Fatal("generated fallback reused a bearer key")
		}
		for _, key := range []string{first, second} {
			decoded, decodeErr := hex.DecodeString(key)
			if decodeErr != nil || len(decoded) != 32 {
				t.Fatalf("generated key %q is not 32 random bytes encoded as hex", key)
			}
		}
	})
}

func TestWriteDaemonMCPKeyEnforcesPrivateMode(t *testing.T) {
	path := filepath.Join(t.TempDir(), "mcp.key")
	if err := os.WriteFile(path, []byte("old-key"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatal(err)
	}

	if err := writeDaemonMCPKey(path, "active-key"); err != nil {
		t.Fatal(err)
	}
	if !daemonMCPKeyFileMatches(path, "active-key") {
		t.Fatal("written key or file mode does not match the active daemon key")
	}
	if runtime.GOOS == "windows" {
		return
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if got := info.Mode().Perm(); got != 0o600 {
		t.Fatalf("mcp.key mode = %o, want 600", got)
	}
}

func TestSplitLines(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		want  int
	}{
		{"empty", "", 0},
		{"single_line_no_newline", "hello", 1},
		{"single_line_with_newline", "hello\n", 1},
		{"two_lines", "hello\nworld", 2},
		{"two_lines_trailing", "hello\nworld\n", 2},
		{"multiple_empty_lines", "\n\n\n", 3},
		{"mixed", "line1\nline2\nline3", 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := splitLines(tt.input)
			if len(got) != tt.want {
				t.Errorf("splitLines(%q) = %d lines; want %d (lines=%v)", tt.input, len(got), tt.want, got)
			}
		})
	}
}

func TestSplitLastN(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		n     int
		want  int
	}{
		{"empty", "", 5, 0},
		{"fewer_than_n", "line1\nline2\n", 5, 2},
		{"exact_n", "line1\nline2\nline3\n", 3, 3},
		{"more_than_n", "line1\nline2\nline3\nline4\nline5\n", 3, 3},
		{"n_is_1", "a\nb\nc\n", 1, 1},
		{"skips_empty_lines", "line1\n\nline2\n\nline3\n", 10, 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := splitLastN(tt.input, tt.n)
			if len(got) != tt.want {
				t.Errorf("splitLastN(%q, %d) = %d lines; want %d (lines=%v)", tt.input, tt.n, len(got), tt.want, got)
			}
		})
	}

	t.Run("returns_last_entries", func(t *testing.T) {
		t.Parallel()
		got := splitLastN("a\nb\nc\nd\ne\n", 2)
		if len(got) != 2 {
			t.Fatalf("expected 2 lines, got %d", len(got))
		}
		if got[0] != "d" || got[1] != "e" {
			t.Errorf("expected [d e], got %v", got)
		}
	})
}
