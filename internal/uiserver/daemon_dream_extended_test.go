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
)

func TestHandleDaemonStatus_WithRunningDaemon(t *testing.T) {
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	daemonDir := filepath.Join(tmpHome, "."+brand.Brand, "daemon")
	if err := os.MkdirAll(daemonDir, 0o755); err != nil {
		t.Fatal(err)
	}
	pidFile := daemon.NewPIDFile()
	if err := pidFile.Acquire(); err != nil {
		t.Fatal(err)
	}
	defer pidFile.Release()
	myPID := os.Getpid()

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
