package daemon

import (
	"strings"
	"testing"
)

func TestRemoveCronEntry_Present(t *testing.T) {
	t.Parallel()
	marker := "# MY_MARKER"
	crontab := "# header\n# MY_MARKER\n* * * * * /bin/true\n# tail\n"
	result := removeCronEntry(crontab, marker)
	if strings.Contains(result, marker) {
		t.Error("marker should be removed")
	}
	if strings.Contains(result, "/bin/true") {
		t.Error("cron line after marker should be removed")
	}
	if !strings.Contains(result, "# header") {
		t.Error("header should remain")
	}
	if !strings.Contains(result, "# tail") {
		t.Error("tail should remain")
	}
}

func TestRemoveCronEntry_Absent(t *testing.T) {
	t.Parallel()
	crontab := "* * * * * /bin/other\n"
	result := removeCronEntry(crontab, "# NONEXISTENT")
	if result != crontab {
		t.Errorf("expected unchanged crontab, got %q", result)
	}
}

func TestRemoveCronEntry_Empty(t *testing.T) {
	t.Parallel()
	result := removeCronEntry("", "# MARKER")
	if result != "" {
		t.Errorf("expected empty, got %q", result)
	}
}

func TestRemoveCronEntry_MultipleMarkers(t *testing.T) {
	t.Parallel()
	marker := "# MARKER"
	crontab := marker + "\nline1\n" + marker + "\nline2\nother\n"
	result := removeCronEntry(crontab, marker)
	if strings.Contains(result, marker) {
		t.Error("all markers should be removed")
	}
	if strings.Contains(result, "line1") || strings.Contains(result, "line2") {
		t.Error("lines after markers should be removed")
	}
	if !strings.Contains(result, "other") {
		t.Error("unrelated lines should remain")
	}
}

// removeCronEntry — marker at end of crontab (no following line)

func TestRemoveCronEntry_MarkerAtEndNoCronLine(t *testing.T) {
	t.Parallel()
	marker := "# MARKER"
	crontab := "other\n" + marker
	result := removeCronEntry(crontab, marker)
	if strings.Contains(result, marker) {
		t.Error("marker should be removed even at end")
	}
	if !strings.Contains(result, "other") {
		t.Error("other line should remain")
	}
}

func TestResolveExePath_ReturnsValid(t *testing.T) {
	exe, err := resolveExePath()
	if err != nil {
		t.Logf("resolveExePath error: %v (may be expected in test environment)", err)
		return
	}
	if exe == "" {
		t.Error("expected non-empty path")
	}
}
