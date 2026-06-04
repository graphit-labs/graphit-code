package commands

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFormatDuration(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		d    time.Duration
		want string
	}{
		{name: "zero", d: 0, want: "0s"},
		{name: "negative clamps to zero", d: -5 * time.Second, want: "0s"},
		{name: "seconds only", d: 45 * time.Second, want: "45s"},
		{name: "one minute exact", d: time.Minute, want: "1m"},
		{name: "minutes and seconds", d: 3*time.Minute + 15*time.Second, want: "3m15s"},
		{name: "one hour exact", d: time.Hour, want: "1h"},
		{name: "hours and minutes", d: 2*time.Hour + 30*time.Minute, want: "2h30m"},
		{name: "one day exact", d: 24 * time.Hour, want: "1d"},
		{name: "days and hours", d: 3*24*time.Hour + 5*time.Hour, want: "3d5h"},
		{name: "days without hours", d: 2 * 24 * time.Hour, want: "2d"},
		{name: "hours without minutes", d: 5 * time.Hour, want: "5h"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := formatDuration(tt.d)
			if got != tt.want {
				t.Errorf("formatDuration(%v) = %q; want %q", tt.d, got, tt.want)
			}
		})
	}
}

func TestHumanSize(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		bytes int64
		want  string
	}{
		{name: "zero bytes", bytes: 0, want: "0 B"},
		{name: "small bytes", bytes: 512, want: "512 B"},
		{name: "one KB boundary", bytes: 1024, want: "1.0 KB"},
		{name: "fractional KB", bytes: 1536, want: "1.5 KB"},
		{name: "large KB", bytes: 500 * 1024, want: "500.0 KB"},
		{name: "one MB boundary", bytes: 1024 * 1024, want: "1.0 MB"},
		{name: "fractional MB", bytes: 3*1024*1024 + 512*1024, want: "3.5 MB"},
		{name: "just under KB", bytes: 1023, want: "1023 B"},
		{name: "just under MB", bytes: 1024*1024 - 1, want: "1024.0 KB"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := humanSize(tt.bytes)
			if got != tt.want {
				t.Errorf("humanSize(%d) = %q; want %q", tt.bytes, got, tt.want)
			}
		})
	}
}

func TestExtractFrontmatterTitle(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			name:    "valid title",
			content: "---\ntitle: My Report\ndate: 2024-01-01\n---\n# Body",
			want:    "My Report",
		},
		{
			name:    "quoted title",
			content: "---\ntitle: \"Quoted Title\"\n---\n# Body",
			want:    "Quoted Title",
		},
		{
			name:    "single quoted title",
			content: "---\ntitle: 'Single Quoted'\n---\n# Body",
			want:    "Single Quoted",
		},
		{
			name:    "no frontmatter",
			content: "# Just a heading\nSome content",
			want:    "",
		},
		{
			name:    "frontmatter without title",
			content: "---\ndate: 2024-01-01\nauthor: test\n---\nBody",
			want:    "",
		},
		{
			name:    "empty content",
			content: "",
			want:    "",
		},
		{
			name:    "unclosed frontmatter",
			content: "---\ntitle: Orphan\n",
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
			got := extractFrontmatterTitle(path)
			if got != tt.want {
				t.Errorf("extractFrontmatterTitle() = %q; want %q", got, tt.want)
			}
		})
	}

	t.Run("nonexistent file", func(t *testing.T) {
		t.Parallel()
		got := extractFrontmatterTitle(filepath.Join(dir, "nonexistent.md"))
		if got != "" {
			t.Errorf("expected empty for nonexistent file, got %q", got)
		}
	})
}

func TestScanDreamReports(t *testing.T) {
	t.Parallel()

	t.Run("empty directory", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		entries, err := scanDreamReports(dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(entries) != 0 {
			t.Errorf("expected 0 entries, got %d", len(entries))
		}
	})

	t.Run("nonexistent directory", func(t *testing.T) {
		t.Parallel()
		_, err := scanDreamReports("/nonexistent-dir-graphit-test-xyz")
		if err == nil {
			t.Error("expected error for nonexistent directory")
		}
	})

	t.Run("with report files", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()

		// .md report file with frontmatter
		content := "---\ntitle: Dream Session 1\n---\n# Report body"
		if err := os.WriteFile(filepath.Join(dir, "abc123.md"), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}

		// .md report file without frontmatter
		if err := os.WriteFile(filepath.Join(dir, "def456.md"), []byte("# No frontmatter"), 0o644); err != nil {
			t.Fatal(err)
		}

		// non-.md file (should be skipped)
		if err := os.WriteFile(filepath.Join(dir, "notes.txt"), []byte("text"), 0o644); err != nil {
			t.Fatal(err)
		}

		// subdirectory (should be skipped)
		if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o755); err != nil {
			t.Fatal(err)
		}

		entries, err := scanDreamReports(dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(entries) != 2 {
			t.Fatalf("expected 2 entries, got %d", len(entries))
		}

		titleMap := make(map[string]string)
		for _, e := range entries {
			titleMap[e.ID] = e.Title
		}
		if titleMap["abc123"] != "Dream Session 1" {
			t.Errorf("expected title 'Dream Session 1', got %q", titleMap["abc123"])
		}
		if titleMap["def456"] != "" {
			t.Errorf("expected empty title for def456, got %q", titleMap["def456"])
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

		entries, err := scanDreamReports(dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(entries) != 1 {
			t.Fatalf("expected 1 entry, got %d", len(entries))
		}
		if !entries[0].HasDeepSleep {
			t.Error("expected HasDeepSleep=true for entry with .exhausted sentinel")
		}
	})
}

func TestLoadAndSaveDreamLastSeen(t *testing.T) {
	t.Run("missing file returns zero time", func(t *testing.T) {
		dir := t.TempDir()
		ls := loadDreamLastSeen(dir)
		if !ls.LastViewed.IsZero() {
			t.Errorf("expected zero time, got %v", ls.LastViewed)
		}
	})

	t.Run("invalid JSON returns zero time", func(t *testing.T) {
		dir := t.TempDir()
		seenPath := dreamLastSeenPath(dir)
		if err := os.MkdirAll(filepath.Dir(seenPath), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(seenPath, []byte("not json"), 0o644); err != nil {
			t.Fatal(err)
		}
		ls := loadDreamLastSeen(dir)
		if !ls.LastViewed.IsZero() {
			t.Errorf("expected zero time for invalid JSON, got %v", ls.LastViewed)
		}
	})

	t.Run("valid file roundtrip", func(t *testing.T) {
		dir := t.TempDir()
		seenPath := dreamLastSeenPath(dir)
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

		loaded := loadDreamLastSeen(dir)
		if !loaded.LastViewed.Equal(now) {
			t.Errorf("loaded time %v != written time %v", loaded.LastViewed, now)
		}
	})
}

func TestSaveDreamLastSeen(t *testing.T) {
	dir := t.TempDir()

	before := time.Now().Add(-time.Second)
	saveDreamLastSeen(dir)
	after := time.Now().Add(time.Second)

	ls := loadDreamLastSeen(dir)
	if ls.LastViewed.Before(before) || ls.LastViewed.After(after) {
		t.Errorf("saved time %v not within expected range [%v, %v]", ls.LastViewed, before, after)
	}
}
