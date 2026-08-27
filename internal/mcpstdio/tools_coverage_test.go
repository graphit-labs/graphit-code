package mcpstdio

import (
	"os"
	"path/filepath"
	"testing"
)

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

func TestCopyDirRecursive(t *testing.T) {
	t.Parallel()

	t.Run("copy files and subdirectories", func(t *testing.T) {
		t.Parallel()
		src := t.TempDir()
		dst := filepath.Join(t.TempDir(), "dest")

		if err := os.MkdirAll(filepath.Join(src, "subdir"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(src, "file1.txt"), []byte("content1"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(src, "subdir", "file2.txt"), []byte("content2"), 0o644); err != nil {
			t.Fatal(err)
		}

		if err := copyDirRecursive(src, dst); err != nil {
			t.Fatalf("copyDirRecursive error: %v", err)
		}

		// Verify files
		data1, err := os.ReadFile(filepath.Join(dst, "file1.txt"))
		if err != nil {
			t.Fatalf("file1.txt not copied: %v", err)
		}
		if string(data1) != "content1" {
			t.Errorf("file1.txt = %q; want %q", data1, "content1")
		}

		data2, err := os.ReadFile(filepath.Join(dst, "subdir", "file2.txt"))
		if err != nil {
			t.Fatalf("subdir/file2.txt not copied: %v", err)
		}
		if string(data2) != "content2" {
			t.Errorf("subdir/file2.txt = %q; want %q", data2, "content2")
		}
	})

	t.Run("nonexistent source returns error", func(t *testing.T) {
		t.Parallel()
		dst := filepath.Join(t.TempDir(), "dest")
		err := copyDirRecursive("/nonexistent-src-dir-test-xyz", dst)
		if err == nil {
			t.Error("expected error for nonexistent source")
		}
	})

	t.Run("empty source directory", func(t *testing.T) {
		t.Parallel()
		src := t.TempDir()
		dst := filepath.Join(t.TempDir(), "dest")

		if err := copyDirRecursive(src, dst); err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		info, err := os.Stat(dst)
		if err != nil {
			t.Fatalf("dest dir not created: %v", err)
		}
		if !info.IsDir() {
			t.Error("dest should be a directory")
		}
	})
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
