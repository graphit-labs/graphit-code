package mcpstdio

import "testing"

func TestResolveWikiDir(t *testing.T) {
	t.Run("unknown module returns empty", func(t *testing.T) {
		tmp := t.TempDir()
		got := resolveWikiDir("nonexistent", tmp, "")
		if got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})
}

func TestLoadProjectConfig_NoLockfile(t *testing.T) {
	tmp := t.TempDir()
	cfg := loadProjectConfig(tmp)
	if cfg != nil {
		t.Errorf("expected nil config for dir without lockfile, got %v", cfg)
	}
}

func TestLoadProjectLockInfo_NoLockfile(t *testing.T) {
	tmp := t.TempDir()
	cfg, ides := loadProjectLockInfo(tmp)
	if cfg != nil {
		t.Errorf("expected nil config, got %v", cfg)
	}
	if ides != nil {
		t.Errorf("expected nil ides, got %v", ides)
	}
}

func TestResolveIDEFromProject(t *testing.T) {
	t.Run("no lockfile returns default", func(t *testing.T) {
		tmp := t.TempDir()
		got := resolveIDEFromProject("", tmp)
		// Without a lockfile, the IDE resolution falls back to defaults
		if got == "" {
			t.Error("expected non-empty IDE default")
		}
	})
}
