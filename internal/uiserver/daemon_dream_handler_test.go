package uiserver

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
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
	if res.PIDFilePath == "" {
		t.Error("expected non-empty pid_file_path")
	}
	t.Logf("scheduler_status: %q, pid_file_path: %q", res.SchedulerStatus, res.PIDFilePath)
}

func TestAdvertisedMCPHost(t *testing.T) {
	tests := []struct {
		name        string
		bindHost    string
		requestHost string
		want        string
	}{
		{name: "explicit bind address", bindHost: "127.0.0.1", requestHost: "graphit.example:8080", want: "127.0.0.1"},
		{name: "wildcard IPv4", bindHost: "0.0.0.0", requestHost: "graphit.example:8080", want: "graphit.example"},
		{name: "wildcard IPv6", bindHost: "::", requestHost: "[2001:db8::4]:8080", want: "2001:db8::4"},
		{name: "request without port", bindHost: "0.0.0.0", requestHost: "graphit.internal", want: "graphit.internal"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := advertisedMCPHost(tt.bindHost, tt.requestHost); got != tt.want {
				t.Fatalf("advertisedMCPHost(%q, %q) = %q; want %q", tt.bindHost, tt.requestHost, got, tt.want)
			}
		})
	}
}

func TestDaemonStopRouteIsNotRegistered(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodPost, "/api/daemon/stop", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Errorf("status = %d; want %d", w.Code, http.StatusNotFound)
	}
}

func TestDreamReportsRouteIsNotRegistered(t *testing.T) {
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/reports?project_dir=/tmp", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)
	if w.Code != http.StatusNotFound {
		t.Fatalf("legacy reports route status = %d; want %d", w.Code, http.StatusNotFound)
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

	endpoints := []struct {
		method string
		path   string
	}{
		{http.MethodGet, "/api/daemon/status"},
		{http.MethodGet, "/api/dream/status?project_dir=/tmp"},
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
