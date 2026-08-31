package uiserver

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/backlog"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/dream"
)

func TestHandleDaemonStatus_WithRunningDaemon(t *testing.T) {
	// Set HOME to a temp dir so daemon.NewPIDFile() picks up our fake PID file
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	// Write a PID file containing our own PID (which is alive)
	daemonDir := filepath.Join(tmpHome, "."+brand.Brand, "daemon")
	if err := os.MkdirAll(daemonDir, 0o755); err != nil {
		t.Fatal(err)
	}
	myPID := os.Getpid()
	pidContent := fmt.Sprintf("%d\n%s\n", myPID, time.Now().UTC().Format(time.RFC3339))
	if err := os.WriteFile(filepath.Join(daemonDir, "daemon.pid"), []byte(pidContent), 0o600); err != nil {
		t.Fatal(err)
	}

	// Also create a fake daemon.log to test the RecentLogs path
	logContent := "line1\nline2\nline3\n"
	if err := os.WriteFile(filepath.Join(daemonDir, "daemon.log"), []byte(logContent), 0o644); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/daemon/status", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		PID           int      `json:"pid"`
		Running       bool     `json:"running"`
		StartedAt     string   `json:"started_at"`
		UptimeSeconds int64    `json:"uptime_seconds"`
		PIDFilePath   string   `json:"pid_file_path"`
		RecentLogs    []string `json:"recent_logs"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}

	if !res.Running {
		t.Error("expected Running=true for our own PID")
	}
	if res.PID != myPID {
		t.Errorf("PID = %d; want %d", res.PID, myPID)
	}
	if res.StartedAt == "" {
		t.Error("expected non-empty StartedAt")
	}
	if res.UptimeSeconds < 0 {
		t.Errorf("UptimeSeconds = %d; expected >= 0", res.UptimeSeconds)
	}
	if len(res.RecentLogs) != 3 {
		t.Errorf("RecentLogs length = %d; want 3", len(res.RecentLogs))
	}
}

func TestHandleDaemonStatus_WithStalePID(t *testing.T) {
	// Set HOME to a temp dir, write a PID for a non-existent process
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	daemonDir := filepath.Join(tmpHome, "."+brand.Brand, "daemon")
	if err := os.MkdirAll(daemonDir, 0o755); err != nil {
		t.Fatal(err)
	}
	// Use a very high PID that shouldn't exist
	pidContent := fmt.Sprintf("99999999\n%s\n", time.Now().UTC().Format(time.RFC3339))
	if err := os.WriteFile(filepath.Join(daemonDir, "daemon.pid"), []byte(pidContent), 0o600); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/daemon/status", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		Running bool `json:"running"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}

	if res.Running {
		t.Error("expected Running=false for stale/non-existent PID")
	}
}

func TestHandleDaemonStop_StalePIDFile(t *testing.T) {
	// Write a PID file with a non-existent PID, then try to stop
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	daemonDir := filepath.Join(tmpHome, "."+brand.Brand, "daemon")
	if err := os.MkdirAll(daemonDir, 0o755); err != nil {
		t.Fatal(err)
	}
	pidContent := fmt.Sprintf("99999999\n%s\n", time.Now().UTC().Format(time.RFC3339))
	if err := os.WriteFile(filepath.Join(daemonDir, "daemon.pid"), []byte(pidContent), 0o600); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodPost, "/api/daemon/stop", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	// Since the PID is dead, IsAlive() returns nil, so it's like "not running"
	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res map[string]any
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if res["success"] != true {
		t.Errorf("expected success true, got %v", res["success"])
	}
}

func TestHandleDaemonStop_NoPIDFile(t *testing.T) {
	// HOME is in empty temp dir, no PID file
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

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
		t.Fatalf("decode: %v", err)
	}
	if res["success"] != true {
		t.Errorf("expected success true (no daemon running)")
	}
	if msg, ok := res["message"].(string); ok {
		if !strings.Contains(msg, "No daemon running") {
			t.Errorf("message = %q; expected 'No daemon running'", msg)
		}
	}
}

func TestHandleDreamStatus_WithDreamConfig(t *testing.T) {
	tmp := t.TempDir()

	lockContent := []byte(`{
		"project": {
			"id": "test-id",
			"name": "test-project"
		},
		"config": {
			"dream": {
				"enabled": "false",
				"idle_timeout": "1800",
				"max_duration": "0"
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
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var res struct {
		Enabled     bool   `json:"enabled"`
		MaxDuration string `json:"max_duration"`
		Status      string `json:"status"`
	}
	if err := json.NewDecoder(w.Body).Decode(&res); err != nil {
		t.Fatalf("decode: %v", err)
	}

	if res.Enabled {
		t.Error("expected Enabled=false")
	}
	if res.MaxDuration != "unlimited" {
		t.Errorf("MaxDuration = %q; want %q", res.MaxDuration, "unlimited")
	}
	if res.Status != "inactive" {
		t.Errorf("Status = %q; want %q", res.Status, "inactive")
	}
}

func TestHandleDreamStatus_WithDreamReportsCount(t *testing.T) {
	tmp := t.TempDir()

	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 5; i++ {
		name := fmt.Sprintf("session%d.md", i)
		content := fmt.Sprintf("---\ntitle: Report %d\n---\n# Report %d", i, i)
		if err := os.WriteFile(filepath.Join(dreamDir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
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

	if res.TotalReports != 5 {
		t.Errorf("TotalReports = %d; want 5", res.TotalReports)
	}
}

func TestHandleBacklogList_WithSubjects(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	for i := 0; i < 3; i++ {
		_, err := backlog.Add(tmp, fmt.Sprintf("Subject %d", i), fmt.Sprintf("Body %d", i))
		if err != nil {
			t.Fatalf("backlog.Add %d: %v", i, err)
		}
	}

	h := NewDaemonBacklogHandler()
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/backlog?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var subjects []map[string]any
	if err := json.NewDecoder(w.Body).Decode(&subjects); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(subjects) != 3 {
		t.Errorf("expected 3 subjects, got %d", len(subjects))
	}
}

func TestHandleBacklogAdd_LongTitle(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	h := NewDaemonBacklogHandler()
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	longTitle := strings.Repeat("A very long title ", 20)
	body := fmt.Sprintf(`{"title":%q,"body":"content"}`, longTitle)
	req := httptest.NewRequest(http.MethodPost, "/api/backlog/item?project_dir="+tmp, strings.NewReader(body))
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}
}

func TestHandleBacklogAdd_NoBody(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	h := NewDaemonBacklogHandler()
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	// Title present but no body field (should still work since body is optional)
	body := `{"title":"Title Only"}`
	req := httptest.NewRequest(http.MethodPost, "/api/backlog/item?project_dir="+tmp, strings.NewReader(body))
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}
}

func TestHandleBacklogRemove_EmptySlug(t *testing.T) {
	t.Parallel()
	h := NewDaemonBacklogHandler()
	mux := http.NewServeMux()
	// Register without pattern to test slug validation
	mux.HandleFunc("/test/dream/subject/{slug}", corsJSON(h.handleBacklogRemove))

	// Empty slug path param
	req := httptest.NewRequest(http.MethodDelete, "/test/dream/subject/?project_dir=/tmp", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	// Empty slug should return 404 (no route match) or bad request
	if w.Code == http.StatusOK {
		t.Error("expected error for empty slug")
	}
}

func TestHandleDreamReports_SortedByDate(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// Create reports with different mod times
	now := time.Now()
	for i := 0; i < 3; i++ {
		name := fmt.Sprintf("session%d.md", i)
		content := fmt.Sprintf("---\ntitle: Report %d\n---\n# Report %d", i, i)
		filePath := filepath.Join(dreamDir, name)
		if err := os.WriteFile(filePath, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		// Set different mod times
		modTime := now.Add(time.Duration(i) * time.Hour)
		if err := os.Chtimes(filePath, modTime, modTime); err != nil {
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
		t.Fatalf("expected 3 reports, got %d", len(reports))
	}

	// Should be sorted by created date descending (newest first)
	for i := 1; i < len(reports); i++ {
		if reports[i].Created.After(reports[i-1].Created) {
			t.Errorf("reports not sorted by date descending at index %d", i)
		}
	}
}

func TestHandleDreamReports_MixedContentTypes(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	dreamDir := dream.ReportsDir(tmp)
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// One valid report, one with no frontmatter, non-md files
	if err := os.WriteFile(filepath.Join(dreamDir, "with-title.md"), []byte("---\ntitle: \"Titled\"\n---\n# Content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dreamDir, "no-title.md"), []byte("# Just heading, no frontmatter"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dreamDir, "state.json"), []byte("{}"), 0o644); err != nil {
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
	if len(reports) != 2 {
		t.Fatalf("expected 2 reports (only .md files), got %d", len(reports))
	}

	// Check that one has a title and the other doesn't
	titles := map[string]bool{}
	for _, r := range reports {
		titles[r.Title] = true
	}
	if !titles["Titled"] {
		t.Error("expected report with title 'Titled'")
	}
}

func TestSplitLastNLocal_ZeroN(t *testing.T) {
	t.Parallel()
	res := splitLastNLocal("a\nb\nc\n", 0)
	if len(res) != 0 {
		t.Errorf("expected 0 lines for n=0, got %d", len(res))
	}
}

func TestSplitLastNLocal_SingleNewline(t *testing.T) {
	t.Parallel()
	res := splitLastNLocal("\n", 5)
	// "\n" splits to ["", ""], trim trailing empty → [""]
	if len(res) != 1 {
		t.Errorf("expected 1 line for single newline, got %d: %v", len(res), res)
	}
}

func TestSplitLastNLocal_LargeN(t *testing.T) {
	t.Parallel()
	res := splitLastNLocal("a\nb\nc\n", 1000)
	if len(res) != 3 {
		t.Errorf("expected 3 lines, got %d", len(res))
	}
}

func TestLoadProjectIDNames_MalformedJSON(t *testing.T) {
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	globalDir := filepath.Join(tmpHome, "."+brand.Brand)
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}
	// Truncated JSON
	if err := os.WriteFile(filepath.Join(globalDir, "hub.registry.json"), []byte(`{"projects": {"p1": {"name":`), 0o644); err != nil {
		t.Fatal(err)
	}

	names := loadProjectIDNames()
	if len(names) != 0 {
		t.Errorf("expected empty map for malformed JSON, got %d entries", len(names))
	}
}

func TestLoadProjectIDNames_EmptyProjects(t *testing.T) {
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	globalDir := filepath.Join(tmpHome, "."+brand.Brand)
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(globalDir, "hub.registry.json"), []byte(`{"projects":{}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	names := loadProjectIDNames()
	if len(names) != 0 {
		t.Errorf("expected empty map for empty projects, got %d entries", len(names))
	}
}
