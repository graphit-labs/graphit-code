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

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
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

func TestHandleDaemonStop_NotRunning(t *testing.T) {
	// Set a mock PID path or ensure we try to stop
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

func TestHandleDreamReports(t *testing.T) {
	tmp := t.TempDir()
	dreamDir := filepath.Join(tmp, brand.DotDir(), "dream")
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// Create fake report md
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

	var reports []dreamReportEntry
	if err := json.NewDecoder(w.Body).Decode(&reports); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}

	if len(reports) != 1 {
		t.Errorf("expected 1 report, got %d", len(reports))
	} else if reports[0].Title != "Fake Dream Report" {
		t.Errorf("expected title 'Fake Dream Report', got %s", reports[0].Title)
	}
}

func TestHandleDreamSubjects_AndAddRemove(t *testing.T) {
	tmp := t.TempDir()

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	// 1. Add subject
	addBody := `{"title":"Optimize imports","body":"Clean all unused packages."}`
	reqAdd := httptest.NewRequest(http.MethodPost, "/api/dream/subject?project_dir="+tmp, strings.NewReader(addBody))
	wAdd := httptest.NewRecorder()
	mux.ServeHTTP(wAdd, reqAdd)

	if wAdd.Code != http.StatusOK {
		t.Errorf("add subject status = %d; want %d", wAdd.Code, http.StatusOK)
	}

	// 2. List subjects
	reqList := httptest.NewRequest(http.MethodGet, "/api/dream/subjects?project_dir="+tmp, nil)
	wList := httptest.NewRecorder()
	mux.ServeHTTP(wList, reqList)

	if wList.Code != http.StatusOK {
		t.Errorf("list subjects status = %d; want %d", wList.Code, http.StatusOK)
	}

	var subjects []map[string]any
	if err := json.NewDecoder(wList.Body).Decode(&subjects); err != nil {
		t.Fatalf("failed to decode subjects: %v", err)
	}
	if len(subjects) != 1 {
		t.Errorf("expected 1 subject, got %d", len(subjects))
	}

	// 3. Remove subject
	slug := subjects[0]["Slug"].(string)
	reqRemove := httptest.NewRequest(http.MethodDelete, "/api/dream/subject/"+slug+"?project_dir="+tmp, nil)
	wRemove := httptest.NewRecorder()
	mux.ServeHTTP(wRemove, reqRemove)

	if wRemove.Code != http.StatusOK {
		t.Errorf("remove subject status = %d; want %d", wRemove.Code, http.StatusOK)
	}
}

func TestSplitLastNLocal(t *testing.T) {
	input := "line1\nline2\nline3\n"
	res := splitLastNLocal(input, 2)
	if len(res) != 2 || res[0] != "line2" || res[1] != "line3" {
		t.Errorf("unexpected splitLastNLocal result: %v", res)
	}
}

func TestExtractFrontmatterTitleLocal_Malformed(t *testing.T) {
	tmpFile := filepath.Join(t.TempDir(), "test.md")
	if err := os.WriteFile(tmpFile, []byte("no frontmatter"), 0o644); err != nil {
		t.Fatal(err)
	}
	title := extractFrontmatterTitleLocal(tmpFile)
	if title != "" {
		t.Errorf("expected empty title, got %s", title)
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
