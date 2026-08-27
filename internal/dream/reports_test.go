package dream

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// writeReport puts a report file in the project's reports directory.
func writeReport(t *testing.T, projectDir, name, content string) string {
	t.Helper()
	dir := ReportsDir(projectDir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestReportsDir(t *testing.T) {
	t.Parallel()
	got := ReportsDir("/tmp/testproj")
	want := brand.ProjectRuntimePath("/tmp/testproj", "dream")
	if got != want {
		t.Errorf("ReportsDir() = %q, want %q", got, want)
	}
}

func TestReportTitle(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	tests := []struct {
		name    string
		content string
		want    string
	}{
		{"with title", "---\ntitle: Dream Session 1\n---\nBody", "Dream Session 1"},
		{"double quoted title", "---\ntitle: \"Quoted\"\n---\nBody", "Quoted"},
		{"single quoted title", "---\ntitle: 'Quoted'\n---\nBody", "Quoted"},
		{"title after other fields", "---\ndate: 2024-01-01\ntitle: After Other\ntags: [x]\n---\nBody", "After Other"},
		{"no frontmatter", "# Just a heading\nSome content", ""},
		{"frontmatter without title", "---\ndate: 2024-01-01\nauthor: test\n---\nBody", ""},
		{"empty content", "", ""},
		{"unclosed frontmatter", "---\ntitle: Orphan\n", ""},
		{"tags but no title", "---\ntags: [a, b]\n---\n# Content", ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			path := filepath.Join(dir, tt.name+".md")
			if err := os.WriteFile(path, []byte(tt.content), 0o644); err != nil {
				t.Fatal(err)
			}
			if got := reportTitle(path); got != tt.want {
				t.Errorf("reportTitle() = %q; want %q", got, tt.want)
			}
		})
	}

	t.Run("nonexistent file", func(t *testing.T) {
		t.Parallel()
		if got := reportTitle(filepath.Join(dir, "nonexistent.md")); got != "" {
			t.Errorf("expected empty for nonexistent file, got %q", got)
		}
	})
}

func TestListReports(t *testing.T) {
	t.Parallel()

	// A project that never dreamed has no reports directory. That is not an
	// error — it is the normal state before the first session.
	t.Run("missing reports dir", func(t *testing.T) {
		t.Parallel()
		reports, err := ListReports(t.TempDir())
		if err != nil {
			t.Fatalf("expected nil error for missing dir, got %v", err)
		}
		if reports != nil {
			t.Errorf("expected nil reports, got %v", reports)
		}
	})

	t.Run("empty reports dir", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()
		if err := os.MkdirAll(ReportsDir(projectDir), 0o755); err != nil {
			t.Fatal(err)
		}
		reports, err := ListReports(projectDir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(reports) != 0 {
			t.Errorf("expected 0 reports, got %d", len(reports))
		}
	})

	t.Run("unreadable reports dir errors", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()
		dir := ReportsDir(projectDir)
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
		_ = os.Chmod(dir, 0o000)
		defer func() { _ = os.Chmod(dir, 0o755) }()

		if _, err := ListReports(projectDir); err == nil {
			t.Error("expected an error when the reports dir cannot be read")
		}
	})

	t.Run("skips non-md files and directories", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()

		writeReport(t, projectDir, "abc123.md", "---\ntitle: Dream Session 1\n---\n# Report body")
		writeReport(t, projectDir, "def456.md", "# No frontmatter")
		writeReport(t, projectDir, "notes.txt", "text")
		if err := os.Mkdir(filepath.Join(ReportsDir(projectDir), "subdir"), 0o755); err != nil {
			t.Fatal(err)
		}

		reports, err := ListReports(projectDir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(reports) != 2 {
			t.Fatalf("expected 2 reports, got %d", len(reports))
		}

		titles := make(map[string]string)
		for _, r := range reports {
			titles[r.ID] = r.Title
		}
		if titles["abc123"] != "Dream Session 1" {
			t.Errorf("expected title 'Dream Session 1', got %q", titles["abc123"])
		}
		if titles["def456"] != "" {
			t.Errorf("expected empty title for def456, got %q", titles["def456"])
		}
	})

	// The last-seen marker is a .json file in the same directory, so it must not
	// be mistaken for a report.
	t.Run("skips the last-seen marker", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()
		writeReport(t, projectDir, "sess1.md", "report")
		MarkReportsSeen(projectDir)

		reports, err := ListReports(projectDir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(reports) != 1 {
			t.Fatalf("expected 1 report, got %d", len(reports))
		}
	})

	t.Run("detects the deep sleep sentinel", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()
		writeReport(t, projectDir, "sess1.md", "report")
		writeReport(t, projectDir, "sess1"+DeepSleepSentinelName(), "")

		reports, err := ListReports(projectDir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(reports) != 1 {
			t.Fatalf("expected 1 report, got %d", len(reports))
		}
		if !reports[0].HasDeepSleep {
			t.Error("expected HasDeepSleep=true for a report with an .exhausted sentinel")
		}
	})

	// Every caller wants newest-first, so the ordering is part of the contract
	// rather than something each of them re-sorts.
	t.Run("newest first", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()
		old := writeReport(t, projectDir, "older.md", "old")
		writeReport(t, projectDir, "newer.md", "new")

		past := time.Now().Add(-2 * time.Hour)
		if err := os.Chtimes(old, past, past); err != nil {
			t.Fatal(err)
		}

		reports, err := ListReports(projectDir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(reports) != 2 {
			t.Fatalf("expected 2 reports, got %d", len(reports))
		}
		if reports[0].ID != "newer" || reports[1].ID != "older" {
			t.Errorf("expected newest first, got %q then %q", reports[0].ID, reports[1].ID)
		}
	})
}

func TestReportsSince(t *testing.T) {
	t.Parallel()

	now := time.Now()
	reports := []Report{
		{ID: "new", Created: now},
		{ID: "old", Created: now.Add(-time.Hour)},
	}

	t.Run("filters older reports", func(t *testing.T) {
		t.Parallel()
		got := ReportsSince(reports, now.Add(-30*time.Minute))
		if len(got) != 1 || got[0].ID != "new" {
			t.Errorf("expected only the newer report, got %v", got)
		}
	})

	t.Run("zero time keeps everything", func(t *testing.T) {
		t.Parallel()
		if got := ReportsSince(reports, time.Time{}); len(got) != 2 {
			t.Errorf("expected 2 reports, got %d", len(got))
		}
	})

	t.Run("future cutoff keeps nothing", func(t *testing.T) {
		t.Parallel()
		if got := ReportsSince(reports, now.Add(time.Hour)); len(got) != 0 {
			t.Errorf("expected 0 reports, got %d", len(got))
		}
	})
}

func TestLastSeenRoundTrip(t *testing.T) {
	t.Parallel()

	t.Run("missing file reads as the zero time", func(t *testing.T) {
		t.Parallel()
		if ls := LoadLastSeen(t.TempDir()); !ls.LastViewed.IsZero() {
			t.Errorf("expected zero time, got %v", ls.LastViewed)
		}
	})

	t.Run("invalid JSON reads as the zero time", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()
		writeReport(t, projectDir, "dream_last_seen.json", "not json")

		if ls := LoadLastSeen(projectDir); !ls.LastViewed.IsZero() {
			t.Errorf("expected zero time for invalid JSON, got %v", ls.LastViewed)
		}
	})

	t.Run("written marker reads back", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()

		now := time.Now().Truncate(time.Second)
		data, err := json.MarshalIndent(LastSeen{LastViewed: now}, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		writeReport(t, projectDir, "dream_last_seen.json", string(data))

		if loaded := LoadLastSeen(projectDir); !loaded.LastViewed.Equal(now) {
			t.Errorf("loaded %v != written %v", loaded.LastViewed, now)
		}
	})

	// MarkReportsSeen creates the directory itself, so it works on a project
	// where the dream module has never run.
	t.Run("MarkReportsSeen creates the directory", func(t *testing.T) {
		t.Parallel()
		projectDir := t.TempDir()

		before := time.Now().Add(-time.Second)
		MarkReportsSeen(projectDir)

		ls := LoadLastSeen(projectDir)
		if ls.LastViewed.Before(before) {
			t.Errorf("expected a fresh timestamp, got %v", ls.LastViewed)
		}
	})
}
