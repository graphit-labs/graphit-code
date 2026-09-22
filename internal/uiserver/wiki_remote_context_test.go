//go:build lancedb

package uiserver

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestWikiRemoteReadsRejectUnqualifiedOrPartialScope(t *testing.T) {
	h := &WikiHandler{}
	for _, rawURL := range []string{
		"/api/wiki/pages?project_id=01HUB&context=artifact",
		"/api/wiki/pages?project_id=01HUB",
		"/api/wiki/pages?context=artifact%401.0.0",
	} {
		recorder := httptest.NewRecorder()
		h.handlePages(recorder, httptest.NewRequest(http.MethodGet, rawURL, nil))
		if recorder.Code != http.StatusBadRequest {
			t.Fatalf("%s: status %d", rawURL, recorder.Code)
		}
		if !strings.Contains(recorder.Body.String(), "required") && !strings.Contains(recorder.Body.String(), "exact") {
			t.Fatalf("%s: unexpected error %q", rawURL, recorder.Body.String())
		}
	}
}
