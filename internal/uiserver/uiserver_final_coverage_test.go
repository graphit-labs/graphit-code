//go:build lancedb

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
)

// isAllowedOrigin — IPv6, various ports, no port

func TestIsAllowedOrigin_Extended(t *testing.T) {
	t.Parallel()
	tests := []struct {
		origin string
		want   bool
	}{
		// IPv6 with various ports
		{"http://[::1]:80", true},
		{"http://[::1]:443", true},
		{"http://[::1]:0", true},
		{"http://[::1]:65535", true},
		// IPv6 without port
		{"http://[::1]", true},
		// localhost with non-standard ports
		{"http://localhost:1", true},
		{"http://localhost:65535", true},
		{"http://localhost:0", true},
		// localhost without port
		{"http://localhost", true},
		// 127.0.0.1 without port
		{"http://127.0.0.1", true},
		// Rejected origins
		{"https://[::1]:3000", false},
		{"http://[::2]:3000", false},
		{"http://0.0.0.0:3000", false},
		{"http://10.0.0.1:3000", false},
		{"http://127.0.0.2:3000", false},
		{"", true}, // same-origin
	}
	for _, tt := range tests {
		t.Run(fmt.Sprintf("origin=%q", tt.origin), func(t *testing.T) {
			t.Parallel()
			got := isAllowedOrigin(tt.origin)
			if got != tt.want {
				t.Errorf("isAllowedOrigin(%q) = %v; want %v", tt.origin, got, tt.want)
			}
		})
	}
}

func TestCorsJSON_OPTIONS_Returns204(t *testing.T) {
	t.Parallel()
	called := false
	handler := corsJSON(func(w http.ResponseWriter, r *http.Request) {
		called = true
		writeJSON(w, map[string]string{"status": "ok"})
	})

	req := httptest.NewRequest(http.MethodOptions, "/test", nil)
	req.Header.Set("Origin", "http://[::1]:9090")
	w := httptest.NewRecorder()
	handler(w, req)

	if w.Code != http.StatusNoContent {
		t.Errorf("OPTIONS status = %d; want %d", w.Code, http.StatusNoContent)
	}
	if called {
		t.Error("inner handler should NOT be called on OPTIONS")
	}
	acao := w.Header().Get("Access-Control-Allow-Origin")
	if acao != "http://[::1]:9090" {
		t.Errorf("ACAO = %q; want %q", acao, "http://[::1]:9090")
	}
}

// handleMultiKeywordSearch — keyword-only search

func TestHandleSearch_FallbackExerciseSnippetBoundaries(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	// Create a short file where query appears near position 0 (start boundary)
	if err := indexPage(t, tmp, "start.md", "xfind is at start"); err != nil {
		t.Fatal(err)
	}
	// Create a file where query appears near the end
	long := strings.Repeat("a", 300) + "xfind" + "b"
	if err := indexPage(t, tmp, "end.md", long); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=xfind", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(results) == 0 {
		t.Error("expected at least 1 result")
	}
	for _, r := range results {
		if r.Snippet == "" {
			t.Errorf("expected non-empty snippet for %q", r.Path)
		}
	}
}

func TestHandleBacklogList_ReturnsEmptyArray(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	// Create the subjects dir but with no files
	subjectsDir := filepath.Join(tmp, ".graphit", "dream", "subjects")
	if err := os.MkdirAll(subjectsDir, 0o755); err != nil {
		t.Fatal(err)
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

	body := strings.TrimSpace(w.Body.String())
	if body != "[]" {
		t.Errorf("expected '[]', got %q", body)
	}
}

func TestHandleDreamReports_NullResultsCoerced(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	// Create dream dir but with only non-md files
	dreamDir := filepath.Join(tmp, ".graphit", "dream")
	if err := os.MkdirAll(dreamDir, 0o755); err != nil {
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

	body := strings.TrimSpace(w.Body.String())
	if body != "[]" {
		t.Errorf("expected '[]', got %q", body)
	}
}

// handleSessions DELETE with valid session

func TestNewWikiHandler(t *testing.T) {
	h := NewWikiHandler(nil)
	if h == nil {
		t.Fatal("NewWikiHandler returned nil")
	}
}

func TestHandleBacklogRemove_EmptySlugParam(t *testing.T) {
	t.Parallel()
	h := NewDaemonBacklogHandler()
	mux := http.NewServeMux()

	// Register without method pattern to test internal slug check
	mux.HandleFunc("/test/subject/{slug}", corsJSON(h.handleBacklogRemove))

	// Request with a "slug" that resolves to empty
	req := httptest.NewRequest(http.MethodDelete, "/test/subject/?project_dir=/tmp", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code == http.StatusOK {
		t.Error("expected error for empty slug")
	}
}

func TestHandleBacklogAdd_MethodValidation(t *testing.T) {
	t.Parallel()
	h := NewDaemonBacklogHandler()
	mux := http.NewServeMux()
	mux.HandleFunc("/test/subject", corsJSON(h.handleBacklogAdd))

	for _, method := range []string{http.MethodGet, http.MethodPut, http.MethodDelete} {
		req := httptest.NewRequest(method, "/test/subject?project_dir=/tmp", nil)
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, req)

		if w.Code != http.StatusMethodNotAllowed {
			t.Errorf("method %s: status = %d; want %d", method, w.Code, http.StatusMethodNotAllowed)
		}
	}
}
