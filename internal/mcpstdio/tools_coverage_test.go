package mcpstdio

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
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

func TestScanDreamReportsLocal(t *testing.T) {
	t.Parallel()

	t.Run("empty directory", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		entries, err := scanDreamReportsLocal(dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(entries) != 0 {
			t.Errorf("expected 0 entries, got %d", len(entries))
		}
	})

	t.Run("nonexistent directory", func(t *testing.T) {
		t.Parallel()
		_, err := scanDreamReportsLocal("/nonexistent-dir-mcp-test-xyz")
		if err == nil {
			t.Error("expected error for nonexistent directory")
		}
	})

	t.Run("mixed files", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()

		if err := os.WriteFile(filepath.Join(dir, "report1.md"), []byte("---\ntitle: Test Report\n---\nbody"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, "report2.md"), []byte("plain content"), 0o644); err != nil {
			t.Fatal(err)
		}
		// Non-.md file (skipped)
		if err := os.WriteFile(filepath.Join(dir, "data.json"), []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
		// Subdirectory (skipped)
		if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
			t.Fatal(err)
		}

		entries, err := scanDreamReportsLocal(dir)
		if err != nil {
			t.Fatal(err)
		}
		if len(entries) != 2 {
			t.Fatalf("expected 2 entries, got %d", len(entries))
		}

		titleMap := make(map[string]string)
		for _, e := range entries {
			titleMap[e.ID] = e.Title
		}
		if titleMap["report1"] != "Test Report" {
			t.Errorf("expected title 'Test Report', got %q", titleMap["report1"])
		}
	})

	t.Run("with exhausted sentinel", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()

		if err := os.WriteFile(filepath.Join(dir, "sess1.md"), []byte("report"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, "sess1.exhausted"), []byte(""), 0o644); err != nil {
			t.Fatal(err)
		}

		entries, err := scanDreamReportsLocal(dir)
		if err != nil {
			t.Fatal(err)
		}
		if len(entries) != 1 {
			t.Fatalf("expected 1 entry, got %d", len(entries))
		}
		if !entries[0].HasDeepSleep {
			t.Error("expected HasDeepSleep=true")
		}
	})
}

func TestExtractFrontmatterTitleLocal(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			name:    "valid title",
			content: "---\ntitle: My Title\ndate: 2024-01-01\n---\nbody",
			want:    "My Title",
		},
		{
			name:    "quoted title",
			content: "---\ntitle: \"Quoted\"\n---\nbody",
			want:    "Quoted",
		},
		{
			name:    "no frontmatter",
			content: "# Heading\nbody",
			want:    "",
		},
		{
			name:    "no title key",
			content: "---\ndate: 2024-01-01\n---\nbody",
			want:    "",
		},
		{
			name:    "unclosed frontmatter",
			content: "---\ntitle: Orphan\n",
			want:    "",
		},
		{
			name:    "empty",
			content: "",
			want:    "",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			path := filepath.Join(dir, tt.name+".md")
			if err := os.WriteFile(path, []byte(tt.content), 0o644); err != nil {
				t.Fatal(err)
			}
			got := extractFrontmatterTitleLocal(path)
			if got != tt.want {
				t.Errorf("extractFrontmatterTitleLocal() = %q; want %q", got, tt.want)
			}
		})
	}

	t.Run("nonexistent file", func(t *testing.T) {
		t.Parallel()
		got := extractFrontmatterTitleLocal("/nonexistent-test-file.md")
		if got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})
}

func TestDreamLastSeenLocal(t *testing.T) {
	t.Run("load from missing file returns zero time", func(t *testing.T) {
		dir := t.TempDir()
		ls := loadDreamLastSeenLocal(dir)
		if !ls.LastViewed.IsZero() {
			t.Errorf("expected zero time, got %v", ls.LastViewed)
		}
	})

	t.Run("load from invalid JSON returns zero time", func(t *testing.T) {
		dir := t.TempDir()
		seenPath := dreamLastSeenPathLocal(dir)
		if err := os.MkdirAll(filepath.Dir(seenPath), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(seenPath, []byte("invalid json"), 0o644); err != nil {
			t.Fatal(err)
		}
		ls := loadDreamLastSeenLocal(dir)
		if !ls.LastViewed.IsZero() {
			t.Errorf("expected zero time, got %v", ls.LastViewed)
		}
	})

	t.Run("save and load roundtrip", func(t *testing.T) {
		dir := t.TempDir()

		before := time.Now().Add(-time.Second)
		saveDreamLastSeenLocal(dir)
		after := time.Now().Add(time.Second)

		ls := loadDreamLastSeenLocal(dir)
		if ls.LastViewed.Before(before) || ls.LastViewed.After(after) {
			t.Errorf("saved time %v not in expected range [%v, %v]", ls.LastViewed, before, after)
		}
	})

	t.Run("valid JSON roundtrip", func(t *testing.T) {
		dir := t.TempDir()
		seenPath := dreamLastSeenPathLocal(dir)
		if err := os.MkdirAll(filepath.Dir(seenPath), 0o755); err != nil {
			t.Fatal(err)
		}

		now := time.Now().Truncate(time.Second)
		ls := dreamLastSeen{LastViewed: now}
		data, err := json.MarshalIndent(ls, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(seenPath, data, 0o644); err != nil {
			t.Fatal(err)
		}

		loaded := loadDreamLastSeenLocal(dir)
		if !loaded.LastViewed.Equal(now) {
			t.Errorf("loaded time %v != written time %v", loaded.LastViewed, now)
		}
	})
}

func TestCopyDirRecursive(t *testing.T) {
	t.Parallel()

	t.Run("copy files and subdirectories", func(t *testing.T) {
		t.Parallel()
		src := t.TempDir()
		dst := filepath.Join(t.TempDir(), "dest")

		// Create source structure
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
