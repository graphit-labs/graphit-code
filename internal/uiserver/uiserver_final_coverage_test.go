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

	"github.com/graphit-labs/graphit-code/internal/chat"
)

// ─── isAllowedOrigin — IPv6, various ports, no port ─────────────────────────

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

// ─── corsJSON OPTIONS returns 204 ───────────────────────────────────────────

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

// ─── handleMultiKeywordSearch — keyword-only search ─────────────────────────

func TestHandleMultiKeywordSearch_KeywordOnlySearch(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	// Create pages with searchable content
	for i := 0; i < 3; i++ {
		name := fmt.Sprintf("doc%d.md", i)
		content := fmt.Sprintf("# Document %d\n\nThis contains specialkeyword%d for testing", i, i)
		if err := os.WriteFile(filepath.Join(tmp, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := fmt.Sprintf(`{"query":"specialkeyword","wiki_dirs":[{"id":"src1","label":"Source 1","dir":%q}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []MultiKeywordResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}

	for _, r := range results {
		if r.SourceID != "src1" {
			t.Errorf("SourceID = %q; want %q", r.SourceID, "src1")
		}
		if r.SourceLabel != "Source 1" {
			t.Errorf("SourceLabel = %q; want %q", r.SourceLabel, "Source 1")
		}
	}
}

// ─── handleMultiKeywordSearch — nil hubSvc with hub_refs ────────────────────

func TestHandleMultiKeywordSearch_NilHubSvcWithHubRefs(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "page.md"), []byte("# Page\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	// hubSvc is nil, hub_refs should be silently skipped
	h := &WikiHandler{hubSvc: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := fmt.Sprintf(`{"query":"content","wiki_dirs":[{"id":"w","label":"W","dir":%q}],"hub_refs":[{"id":"some-artifact","version":"1.0"}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []MultiKeywordResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
}

// ─── handleMultiSearch — nil hubSvc with hub_refs ───────────────────────────

func TestHandleMultiSearch_NilHubSvcWithHubRefs(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "doc.md"), []byte("# Doc\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{aiClient: fakeAIClient{response: "answer"}, hubSvc: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	body := fmt.Sprintf(`{"query":"test","wiki_dirs":[{"id":"w","label":"W","dir":%q}],"hub_refs":[{"id":"artifact","version":""}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
}

// ─── handleSessionMessages — create real session, verify LoadHistory ────────

func TestHandleSessionMessages_WithRealSession(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("HOME", tmp)

	wd, _ := os.Getwd()
	session := chat.NewSession(wd, []chat.WikiSource{
		{ID: "test", Label: "Test", Dir: tmp},
	}, "test query")
	_ = session.Append(chat.ChatMessage{
		Role:    "user",
		Content: "hello",
	})
	_ = session.Append(chat.ChatMessage{
		Role:    "assistant",
		Content: "hi back",
	})

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/wiki/sessions/messages", corsJSON(h.handleSessionMessages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions/messages?id="+session.ID, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d, body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var messages []chat.ChatMessage
	if err := json.NewDecoder(w.Body).Decode(&messages); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(messages) != 2 {
		t.Errorf("expected 2 messages, got %d", len(messages))
	}
}

// ─── handleSearch — exercise fallback search lines 184-218 ──────────────────

func TestHandleSearch_FallbackExerciseSnippetBoundaries(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	// Create a short file where query appears near position 0 (start boundary)
	if err := os.WriteFile(filepath.Join(tmp, "start.md"), []byte("xfind is at start"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Create a file where query appears near the end
	long := strings.Repeat("a", 300) + "xfind" + "b"
	if err := os.WriteFile(filepath.Join(tmp, "end.md"), []byte(long), 0o644); err != nil {
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

// ─── handleDreamSubjects — nil subjects becomes empty array ─────────────────

func TestHandleDreamSubjects_ReturnsEmptyArray(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	// Create the subjects dir but with no files
	subjectsDir := filepath.Join(tmp, ".graphit", "dream", "subjects")
	if err := os.MkdirAll(subjectsDir, 0o755); err != nil {
		t.Fatal(err)
	}

	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/dream/subjects?project_dir="+tmp, nil)
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

// ─── handleDreamReports — null -> empty array ───────────────────────────────

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

// ─── handleSessions DELETE with valid session ───────────────────────────────

func TestHandleSessions_DeleteValidSession(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("HOME", tmp)

	wd, _ := os.Getwd()
	session := chat.NewSession(wd, []chat.WikiSource{
		{ID: "test", Label: "Test", Dir: tmp},
	}, "test query for delete")
	_ = session.Append(chat.ChatMessage{Role: "user", Content: "hello"})

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodDelete, "/api/wiki/sessions?id="+session.ID, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d, body: %s", w.Code, http.StatusOK, w.Body.String())
	}
}

// ─── NewWikiHandler ─────────────────────────────────────────────────────────

func TestNewWikiHandler(t *testing.T) {
	h := NewWikiHandler(nil)
	if h == nil {
		t.Fatal("NewWikiHandler returned nil")
	}
}

// ─── handleDreamSubjectRemove — missing slug ────────────────────────────────

func TestHandleDreamSubjectRemove_EmptySlugParam(t *testing.T) {
	t.Parallel()
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()

	// Register without method pattern to test internal slug check
	mux.HandleFunc("/test/subject/{slug}", corsJSON(h.handleDreamSubjectRemove))

	// Request with a "slug" that resolves to empty
	req := httptest.NewRequest(http.MethodDelete, "/test/subject/?project_dir=/tmp", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code == http.StatusOK {
		t.Error("expected error for empty slug")
	}
}

// ─── handleDreamSubjectAdd — method validation ──────────────────────────────

func TestHandleDreamSubjectAdd_MethodValidation(t *testing.T) {
	t.Parallel()
	h := NewDaemonDreamHandler(nil)
	mux := http.NewServeMux()
	mux.HandleFunc("/test/subject", corsJSON(h.handleDreamSubjectAdd))

	for _, method := range []string{http.MethodGet, http.MethodPut, http.MethodDelete} {
		req := httptest.NewRequest(method, "/test/subject?project_dir=/tmp", nil)
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, req)

		if w.Code != http.StatusMethodNotAllowed {
			t.Errorf("method %s: status = %d; want %d", method, w.Code, http.StatusMethodNotAllowed)
		}
	}
}
