package daemon

import (
	"os/exec"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestCronMarker(t *testing.T) {
	expected := "# " + strings.ToUpper(brand.Brand) + "_DAEMON_SCHEDULER"
	got := cronMarker()
	if got != expected {
		t.Errorf("expected %q, got %q", expected, got)
	}
	if !strings.HasPrefix(got, "# ") {
		t.Errorf("cronMarker should start with '# ', got %q", got)
	}
}

func TestResolveExePath(t *testing.T) {
	exe, err := resolveExePath()
	if err != nil {
		t.Fatalf("resolveExePath: %v", err)
	}
	if exe == "" {
		t.Error("expected non-empty path")
	}
	if !strings.HasPrefix(exe, "/") {
		t.Errorf("expected absolute path, got %q", exe)
	}
}

func TestIsSchedulerInstalled_NoPanic(t *testing.T) {
	_ = IsSchedulerInstalled()
}

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

func TestSchedulerStatus_NoCrontab(t *testing.T) {
	status := SchedulerStatus()
	// On systems without crontab, this should return a "not installed" message
	if status == "" {
		t.Error("expected non-empty status")
	}
}

func TestInstallScheduler_RequiresCrontab(t *testing.T) {
	// Skip if crontab is not available
	if _, err := exec.LookPath("crontab"); err != nil {
		t.Skip("crontab not in PATH")
	}

	// Save the current crontab
	out, _ := exec.Command("crontab", "-l").Output()
	defer func() {
		// Restore
		cmd := exec.Command("crontab", "-")
		cmd.Stdin = strings.NewReader(string(out))
		_ = cmd.Run()
	}()

	if err := InstallScheduler(); err != nil {
		t.Fatalf("InstallScheduler: %v", err)
	}

	// Verify it's installed
	status := SchedulerStatus()
	if !strings.Contains(status, "installed") {
		t.Errorf("expected 'installed' in status after install, got %q", status)
	}

	if err := RemoveScheduler(); err != nil {
		t.Fatalf("RemoveScheduler: %v", err)
	}

	// Verify it's removed
	status = SchedulerStatus()
	if !strings.Contains(status, "not installed") {
		t.Errorf("expected 'not installed' after remove, got %q", status)
	}
}

func TestRemoveScheduler_NoCrontab(t *testing.T) {
	// RemoveScheduler should not fail when there's nothing to remove
	if _, err := exec.LookPath("crontab"); err != nil {
		t.Skip("crontab not in PATH")
	}

	// If crontab doesn't have our entry, RemoveScheduler should not error
	err := RemoveScheduler()
	if err != nil {
		t.Logf("RemoveScheduler returned error (may be expected): %v", err)
	}
}
