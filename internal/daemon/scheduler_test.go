package daemon

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ---------------------------------------------------------------------------
// cronMarker
// ---------------------------------------------------------------------------

func TestCronMarker(t *testing.T) {
	expected := "# " + strings.ToUpper(brand.Brand) + "_DAEMON_SCHEDULER"
	got := cronMarker()
	if got != expected {
		t.Errorf("expected %q, got %q", expected, got)
	}
	// Must start with "# " (valid cron comment)
	if !strings.HasPrefix(got, "# ") {
		t.Errorf("cronMarker should start with '# ', got %q", got)
	}
}

// ---------------------------------------------------------------------------
// resolveExePath — basic validation
// ---------------------------------------------------------------------------

func TestResolveExePath(t *testing.T) {
	exe, err := resolveExePath()
	if err != nil {
		t.Fatalf("resolveExePath: %v", err)
	}
	if exe == "" {
		t.Error("expected non-empty path")
	}
	// Should be an absolute path
	if !strings.HasPrefix(exe, "/") {
		t.Errorf("expected absolute path, got %q", exe)
	}
}

// ---------------------------------------------------------------------------
// removeCronEntry (Linux-only, but the function is pure string manipulation)
// ---------------------------------------------------------------------------

func TestRemoveCronEntry_NoMarker(t *testing.T) {
	crontab := "0 * * * * /usr/bin/command\n5 * * * * /usr/bin/other\n"
	marker := "# TEST_MARKER"
	result := removeCronEntry(crontab, marker)
	if result != crontab {
		t.Errorf("crontab should be unchanged when marker not present.\nexpected: %q\ngot:      %q", crontab, result)
	}
}

func TestRemoveCronEntry_WithMarker(t *testing.T) {
	marker := "# GRAPHIT_DAEMON_SCHEDULER"
	crontab := "0 * * * * /usr/bin/command\n" + marker + "\n* * * * * /usr/bin/graphit daemon\n5 * * * * /usr/bin/other\n"
	result := removeCronEntry(crontab, marker)
	// Should remove the marker line and the line after it
	if strings.Contains(result, marker) {
		t.Error("marker should be removed")
	}
	if strings.Contains(result, "graphit daemon") {
		t.Error("cron entry after marker should be removed")
	}
	if !strings.Contains(result, "/usr/bin/command") {
		t.Error("other entries should be preserved")
	}
	if !strings.Contains(result, "/usr/bin/other") {
		t.Error("other entries should be preserved")
	}
}

func TestRemoveCronEntry_MarkerAtEnd(t *testing.T) {
	marker := "# TEST_MARKER"
	crontab := "0 * * * * /usr/bin/command\n" + marker + "\n* * * * * /usr/bin/graphit daemon"
	result := removeCronEntry(crontab, marker)
	if strings.Contains(result, marker) {
		t.Error("marker should be removed")
	}
	if strings.Contains(result, "graphit daemon") {
		t.Error("cron entry after marker should be removed")
	}
}

func TestRemoveCronEntry_EmptyCrontab(t *testing.T) {
	result := removeCronEntry("", "# TEST_MARKER")
	if result != "" {
		t.Errorf("expected empty string, got %q", result)
	}
}

func TestRemoveCronEntry_OnlyMarker(t *testing.T) {
	marker := "# MARKER"
	crontab := marker + "\n* * * * * /cmd\n"
	result := removeCronEntry(crontab, marker)
	// The trailing empty line from the split may remain
	trimmed := strings.TrimSpace(result)
	if strings.Contains(trimmed, marker) {
		t.Error("marker should be removed")
	}
	if strings.Contains(trimmed, "/cmd") {
		t.Error("command line after marker should be removed")
	}
}

func TestRemoveCronEntry_MultipleOccurrences(t *testing.T) {
	marker := "# MARKER"
	crontab := "0 1 * * * /first\n" +
		marker + "\n* * * * * /graphit1\n" +
		"0 2 * * * /second\n" +
		marker + "\n* * * * * /graphit2\n" +
		"0 3 * * * /third\n"
	result := removeCronEntry(crontab, marker)
	if strings.Contains(result, marker) {
		t.Error("all markers should be removed")
	}
	if strings.Contains(result, "/graphit1") || strings.Contains(result, "/graphit2") {
		t.Error("all graphit entries should be removed")
	}
	if !strings.Contains(result, "/first") || !strings.Contains(result, "/second") || !strings.Contains(result, "/third") {
		t.Error("other entries should be preserved")
	}
}

// ---------------------------------------------------------------------------
// IsSchedulerInstalled — (cannot truly test install without crontab,
// but we can verify the function doesn't panic)
// ---------------------------------------------------------------------------

func TestIsSchedulerInstalled_NoPanic(t *testing.T) {
	// Just verify it doesn't panic — the result depends on actual crontab state
	_ = IsSchedulerInstalled()
}
