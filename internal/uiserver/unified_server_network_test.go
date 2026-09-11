package uiserver

import (
	"net/http/httptest"
	"strings"
	"testing"
)

func TestUnifiedUIUsesSameOriginAPIBase(t *testing.T) {
	s := &UnifiedServer{port: 8080, projectName: "remote-project"}
	req := httptest.NewRequest("GET", "http://remote.example.test/", nil)
	w := httptest.NewRecorder()

	s.handleUI(w, req)
	body := w.Body.String()
	if strings.Contains(body, "http://localhost:8080") {
		t.Fatal("unified UI still injects a localhost API base")
	}
	if !strings.Contains(body, `window.__APP_MODE__ = "unified"`) {
		t.Fatal("unified UI bootstrap was not injected")
	}
}
