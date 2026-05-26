package uiserver

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestUnifiedServerBasic(t *testing.T) {
	s := &UnifiedServer{
		port:        12345,
		projectName: "Test Project",
		mux:         http.NewServeMux(),
	}

	if s.Port() != 12345 {
		t.Errorf("expected port 12345, got %d", s.Port())
	}

	// Test handleUI fallback when UI assets are not built or DistFS is empty
	req := httptest.NewRequest("GET", "/", nil)
	w := httptest.NewRecorder()

	s.handleUI(w, req)

	resp := w.Result()
	if resp.StatusCode != http.StatusInternalServerError && resp.StatusCode != http.StatusOK {
		t.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	body := w.Body.String()
	// Should either serve UI successfully or return "UI not found" error
	if resp.StatusCode == http.StatusInternalServerError && !strings.Contains(body, "UI not found") {
		t.Errorf("unexpected error body: %q", body)
	}
}
