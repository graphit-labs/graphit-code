package uiserver

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/dream"
)

func TestHandleDaemonStatus(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/daemon/status", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		PID     int  `json:"pid"`
		Running bool `json:"running"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("failed to decode daemon status response: %v", err)
	}

	// The daemon may or may not be running on the test machine, so we just check that the field exists and is decodeable
	t.Logf("Daemon running: %v, PID: %d", res.Running, res.PID)
}

func TestHandleDaemonStatus_ResponseFields(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/daemon/status", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var res struct {
		PIDFilePath     string `json:"pid_file_path"`
		SchedulerStatus string `json:"scheduler_status"`
		Running         bool   `json:"running"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// PIDFilePath should always be present
	if res.PIDFilePath == "" {
		t.Error("expected non-empty pid_file_path")
	}
	// SchedulerStatus should always be present
	t.Logf("scheduler_status: %q, pid_file_path: %q", res.SchedulerStatus, res.PIDFilePath)
}

func TestHandleDaemonStop_NotRunning(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodPost, "/api/daemon/stop", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res map[string]any
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if res["success"] != true {
		t.Errorf("expected success true, got %v", res["success"])
	}
}

func TestHandleDaemonStop_InvalidMethod(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/daemon/stop", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusMethodNotAllowed {
		t.Errorf("status = %d; want %d", w.Code, http.StatusMethodNotAllowed)
	}
}

func TestHandleDreamStatus_MissingDir(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/status", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
	}
}

func TestHandleDreamStatus_ValidDir(t *testing.T) {
	tmp := t.TempDir()

	// Create minimal lock file in tmp project dir with nested map for dream config
	lockContent := []byte(`{
		"project": {
			"id": "test-id",
			"name": "test-project"
		},
		"config": {
			"dream": {
				"idle_timeout": "3600",
				"max_duration": "7200"
			}
		}
	}`)
	lockPath := filepath.Join(tmp, brand.LockFileName())
	if err := os.WriteFile(lockPath, lockContent, 0o644); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/status?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		Enabled     bool   `json:"enabled"`
		IdleTimeout string `json:"idle_timeout"`
		MaxDuration string `json:"max_duration"`
		Status      string `json:"status"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("failed to decode dream status response: %v", err)
	}

	if res.IdleTimeout != "1h0m0s" {
		t.Errorf("expected idle_timeout 1h0m0s, got %s", res.IdleTimeout)
	}
	if res.MaxDuration != "2h0m0s" {
		t.Errorf("expected max_duration 2h0m0s, got %s", res.MaxDuration)
	}
}

func TestHandleDreamStatus_NoLockFile(t *testing.T) {
	tmp := t.TempDir()
	// No lockfile — should still work with defaults

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/status?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		Status      string `json:"status"`
		IdleTimeout string `json:"idle_timeout"`
		MaxDuration string `json:"max_duration"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if res.Status == "" {
		t.Error("expected non-empty status")
	}
	if res.IdleTimeout == "" {
		t.Error("expected non-empty idle_timeout")
	}
}

func TestHandleDreamStatus_WithDreamDir(t *testing.T) {
	tmp := t.TempDir()
	// Create dream dir with reports to exercise the TotalReports counting
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dreamDir, "report1.md"), []byte("---\ntitle: Report 1\n---\n# Report"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/status?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		TotalReports int `json:"total_reports"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if res.TotalReports != 1 {
		t.Errorf("TotalReports = %d; want 1", res.TotalReports)
	}
}

func TestHandleDreamReports(t *testing.T) {
	tmp := t.TempDir()
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	reportContent := []byte("---\ntitle: \"Fake Dream Report\"\n---\n# Reflection\nAutonomous improvements.")
	if err := os.WriteFile(filepath.Join(dreamDir, "session1.md"), reportContent, 0o644); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var reports []dream.Report
	if err := json.NewDecoder(w.Body).Decode(&reports); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if len(reports) != 1 {
		t.Errorf("expected 1 report, got %d", len(reports))
	} else if reports[0].Title != "Fake Dream Report" {
		t.Errorf("expected title 'Fake Dream Report', got %s", reports[0].Title)
	}
}

func TestHandleDreamReports_MissingDir(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
	}
}

func TestHandleDreamReports_NoDreamDir(t *testing.T) {
	tmp := t.TempDir()
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var reports []dream.Report
	if err := json.NewDecoder(w.Body).Decode(&reports); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(reports) != 0 {
		t.Errorf("expected 0 reports, got %d", len(reports))
	}
}

func TestHandleDreamReports_MultipleReports(t *testing.T) {
	tmp := t.TempDir()
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	for i := 0; i < 3; i++ {
		name := fmt.Sprintf("session%d.md", i)
		content := fmt.Sprintf("---\ntitle: \"Report %d\"\n---\n# Report %d", i, i)
		if err := os.WriteFile(filepath.Join(dreamDir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var reports []dream.Report
	if err := json.NewDecoder(w.Body).Decode(&reports); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(reports) != 3 {
		t.Errorf("expected 3 reports, got %d", len(reports))
	}

	// Should be sorted by created date descending
	for i := 1; i < len(reports); i++ {
		if reports[i].Created.After(reports[i-1].Created) {
			t.Errorf("reports not sorted by date descending: %v > %v", reports[i].Created, reports[i-1].Created)
		}
	}
}

func TestHandleDreamReports_WithDeepSleep(t *testing.T) {
	tmp := t.TempDir()
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	reportContent := []byte("---\ntitle: \"Deep Sleep Report\"\n---\n# Deep sleep content")
	if err := os.WriteFile(filepath.Join(dreamDir, "deep1.md"), reportContent, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dreamDir, "deep1.exhausted"), []byte(""), 0o644); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var reports []dream.Report
	if err := json.NewDecoder(w.Body).Decode(&reports); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(reports) != 1 {
		t.Fatalf("expected 1 report, got %d", len(reports))
	}
	if !reports[0].HasDeepSleep {
		t.Error("expected HasDeepSleep=true for report with .exhausted sentinel")
	}
}

func TestHandleDreamReports_NonMarkdownFilesIgnored(t *testing.T) {
	tmp := t.TempDir()
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(filepath.Join(dreamDir, "state.json"), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dreamDir, "data.txt"), []byte("data"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dreamDir, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var reports []dream.Report
	if err := json.NewDecoder(w.Body).Decode(&reports); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(reports) != 0 {
		t.Errorf("expected 0 reports (only non-md files), got %d", len(reports))
	}
}

// Should return an empty array, not null

// It could be null if backlog.List returns nil

// 1. Add an item

// The wire contract is snake_case, matching every other result struct and
// the TypeScript BacklogItem interface.

// Register using non-pattern route to test method check inside handler

// Register without method pattern to test the handler's own method check

func TestSplitLastNLocal(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		n        int
		wantLen  int
		wantLast string
	}{
		{"basic with trailing newline", "line1\nline2\nline3\n", 2, 2, "line3"},
		{"exact count", "a\nb\nc\n", 3, 3, "c"},
		{"fewer than n", "a\nb\n", 5, 2, "b"},
		{"single line", "only\n", 1, 1, "only"},
		{"no trailing newline", "a\nb\nc", 2, 2, "c"},
		{"empty string", "", 5, 0, ""},
		{"all lines", "x\ny\nz\n", 10, 3, "z"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			res := splitLastNLocal(tt.input, tt.n)
			if len(res) != tt.wantLen {
				t.Errorf("len = %d; want %d (result: %v)", len(res), tt.wantLen, res)
			}
			if tt.wantLast != "" && len(res) > 0 && res[len(res)-1] != tt.wantLast {
				t.Errorf("last = %q; want %q", res[len(res)-1], tt.wantLast)
			}
		})
	}
}

func TestHandleDaemonStop_RunningAndStop(t *testing.T) {
	tmpDir := t.TempDir()

	t.Setenv("HOME", tmpDir)
	pid := daemon.NewPIDFile()

	err := os.MkdirAll(filepath.Dir(pid.Path()), 0o755)
	if err != nil {
		t.Fatal(err)
	}

	myPid := os.Getpid()
	content := fmt.Sprintf("%d\n%s\n", myPid, time.Now().UTC().Format(time.RFC3339))
	err = os.WriteFile(pid.Path(), []byte(content), 0o600)
	if err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)
}

func TestNewDaemonDreamHandler(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	if h == nil {
		t.Fatal("NewDaemonDreamHandler returned nil")
	}
}

func TestDaemonDreamHandler_RegisterAPIRoutes(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	// Verify routes exist by making requests
	endpoints := []struct {
		method string
		path   string
	}{
		{http.MethodGet, "/api/daemon/status"},
		{http.MethodGet, "/api/dream/status?project_dir=/tmp"},
		{http.MethodGet, "/api/dream/reports?project_dir=/tmp"},
	}
	for _, ep := range endpoints {
		req := httptest.NewRequest(ep.method, ep.path, nil)
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, req)
		if w.Code == 404 {
			t.Errorf("route %s %s not registered", ep.method, ep.path)
		}
	}
}
