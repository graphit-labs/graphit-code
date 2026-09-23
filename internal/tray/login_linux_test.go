//go:build linux

package tray

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTrayLoginEntryIsUserScopedAndRemovable(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(dir, "config"))
	exe := filepath.Join(dir, "Graphit app%", "graphit")
	if err := os.MkdirAll(filepath.Dir(exe), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(exe, []byte("test"), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_LAUNCHER_PATH", exe)
	if err := InstallLogin(); err != nil {
		t.Fatal(err)
	}
	path, err := loginPath()
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `Exec="`+strings.ReplaceAll(exe, "%", "%%")+`" tray`) {
		t.Fatalf("launcher path not escaped: %s", data)
	}
	if enabled, err := LoginEnabled(); err != nil || !enabled {
		t.Fatalf("login enabled=%t err=%v", enabled, err)
	}
	if err := RemoveLogin(); err != nil {
		t.Fatal(err)
	}
	if enabled, err := LoginEnabled(); err != nil || enabled {
		t.Fatalf("login enabled after removal=%t err=%v", enabled, err)
	}
}
