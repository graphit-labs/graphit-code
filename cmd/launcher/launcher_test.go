package main

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCleanupOldRuntimes(t *testing.T) {
	t.Run("removes old version dirs, keeps current", func(t *testing.T) {
		dir := t.TempDir()

		if err := os.MkdirAll(filepath.Join(dir, "v1.0.0"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.MkdirAll(filepath.Join(dir, "v2.0.0"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.MkdirAll(filepath.Join(dir, "v3.0.0"), 0o755); err != nil {
			t.Fatal(err)
		}

		cleanupOldRuntimes(dir, "v2.0.0")

		entries, err := os.ReadDir(dir)
		if err != nil {
			t.Fatal(err)
		}
		if len(entries) != 1 {
			t.Fatalf("expected 1 remaining dir, got %d", len(entries))
		}
		if entries[0].Name() != "v2.0.0" {
			t.Errorf("expected v2.0.0 to remain, got %s", entries[0].Name())
		}
	})

	t.Run("nonexistent base dir does not panic", func(t *testing.T) {
		cleanupOldRuntimes("/nonexistent-cleanup-dir-test-xyz", "v1.0.0")
	})

	t.Run("empty base dir is no-op", func(t *testing.T) {
		dir := t.TempDir()
		cleanupOldRuntimes(dir, "v1.0.0")
		entries, err := os.ReadDir(dir)
		if err != nil {
			t.Fatal(err)
		}
		if len(entries) != 0 {
			t.Errorf("expected 0 entries, got %d", len(entries))
		}
	})

	t.Run("skips files, only removes dirs", func(t *testing.T) {
		dir := t.TempDir()
		if err := os.WriteFile(filepath.Join(dir, "stale-file.txt"), []byte("data"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.MkdirAll(filepath.Join(dir, "old-version"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.MkdirAll(filepath.Join(dir, "current"), 0o755); err != nil {
			t.Fatal(err)
		}

		cleanupOldRuntimes(dir, "current")

		entries, err := os.ReadDir(dir)
		if err != nil {
			t.Fatal(err)
		}
		// stale-file.txt + current dir should remain (file is not IsDir so not processed)
		names := make(map[string]bool)
		for _, e := range entries {
			names[e.Name()] = true
		}
		if !names["current"] {
			t.Error("current dir should remain")
		}
		if !names["stale-file.txt"] {
			t.Error("non-directory file should remain")
		}
		if names["old-version"] {
			t.Error("old-version dir should have been removed")
		}
	})
}

func TestWriteLauncherStamp(t *testing.T) {
	t.Run("creates stamp from binary", func(t *testing.T) {
		appDir := t.TempDir()
		binDir := t.TempDir()
		binPath := filepath.Join(binDir, "test-binary")

		content := []byte("fake binary content for hashing")
		if err := os.WriteFile(binPath, content, 0o755); err != nil {
			t.Fatal(err)
		}

		writeLauncherStamp(appDir, binPath)

		stampPath := filepath.Join(appDir, "daemon", "launcher.stamp")
		data, err := os.ReadFile(stampPath)
		if err != nil {
			t.Fatalf("stamp file not created: %v", err)
		}

		h := sha256.Sum256(content)
		expectedHash := hex.EncodeToString(h[:])

		got := strings.TrimSpace(string(data))
		if got != expectedHash {
			t.Errorf("stamp = %q; want %q", got, expectedHash)
		}
	})

	t.Run("nonexistent binary does not create stamp", func(t *testing.T) {
		appDir := t.TempDir()
		writeLauncherStamp(appDir, filepath.Join(appDir, "nonexistent-bin"))

		stampPath := filepath.Join(appDir, "daemon", "launcher.stamp")
		if _, err := os.Stat(stampPath); err == nil {
			t.Error("stamp file should not be created for nonexistent binary")
		}
	})

	t.Run("stamp file format has trailing newline", func(t *testing.T) {
		appDir := t.TempDir()
		binDir := t.TempDir()
		binPath := filepath.Join(binDir, "bin")

		if err := os.WriteFile(binPath, []byte("bin"), 0o755); err != nil {
			t.Fatal(err)
		}

		writeLauncherStamp(appDir, binPath)

		stampPath := filepath.Join(appDir, "daemon", "launcher.stamp")
		data, err := os.ReadFile(stampPath)
		if err != nil {
			t.Fatal(err)
		}

		if !strings.HasSuffix(string(data), "\n") {
			t.Error("stamp file should end with newline")
		}
	})
}
