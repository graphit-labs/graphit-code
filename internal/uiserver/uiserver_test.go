package uiserver

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestExtractPageMeta(t *testing.T) {
	tests := []struct {
		name       string
		relPath    string
		content    string
		wantTitle  string
		wantType   string
		wantTags   int
		wantSource string
	}{
		{
			name:      "index page",
			relPath:   "index.md",
			content:   "# Main Index\n\nSome content here",
			wantTitle: "Main Index",
			wantType:  "index",
		},
		{
			name:      "log page",
			relPath:   "log.md",
			content:   "# Change Log\n\n- entry 1",
			wantTitle: "Change Log",
			wantType:  "log",
		},
		{
			name:      "community page",
			relPath:   "community-hub.md",
			content:   "# Community Hub\n\nCommunity content",
			wantTitle: "Community Hub",
			wantType:  "community",
		},
		{
			name:      "god-node page",
			relPath:   "god-node-core.md",
			content:   "# God Node Core\n\nCore content",
			wantTitle: "God Node Core",
			wantType:  "god-node",
		},
		{
			name:      "regular entity",
			relPath:   "some-entity.md",
			content:   "# My Entity\n\nDetails here",
			wantTitle: "My Entity",
			wantType:  "entity",
		},
		{
			name:      "no h1 uses filename",
			relPath:   "no-heading.md",
			content:   "Some content without a heading",
			wantTitle: "no-heading",
			wantType:  "entity",
		},
		{
			name:      "with tags",
			relPath:   "tagged.md",
			content:   "---\ntags: [foo, bar, baz]\n---\n# Tagged\n\nContent",
			wantTitle: "Tagged",
			wantType:  "entity",
			wantTags:  3,
		},
		{
			name:       "with source",
			relPath:    "sourced.md",
			content:    "---\nsource: manual\n---\n# Sourced\n\nContent",
			wantTitle:  "Sourced",
			wantType:   "entity",
			wantSource: "manual",
		},
		{
			name:    "with confidence",
			relPath: "confident.md",
			content: "---\nconfidence: 0.95\n---\n# Confident\n\nContent",
		},
		{
			name:    "nested path",
			relPath: "sub/nested-page.md",
			content: "# Nested Page\n\nSub content",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			meta := extractPageMeta(tt.relPath, tt.content)
			if tt.wantTitle != "" && meta.Title != tt.wantTitle {
				t.Errorf("Title = %q; want %q", meta.Title, tt.wantTitle)
			}
			if tt.wantType != "" && meta.Type != tt.wantType {
				t.Errorf("Type = %q; want %q", meta.Type, tt.wantType)
			}
			if tt.wantTags > 0 && len(meta.Tags) != tt.wantTags {
				t.Errorf("Tags count = %d; want %d (tags: %v)", len(meta.Tags), tt.wantTags, meta.Tags)
			}
			if tt.wantSource != "" && meta.Source != tt.wantSource {
				t.Errorf("Source = %q; want %q", meta.Source, tt.wantSource)
			}
			if meta.Path != tt.relPath {
				t.Errorf("Path = %q; want %q", meta.Path, tt.relPath)
			}
		})
	}
}

func TestExtractPageMeta_WordCount(t *testing.T) {
	content := "# Title\n\nOne two three four five"
	meta := extractPageMeta("test.md", content)
	if meta.WordCount < 5 {
		t.Errorf("WordCount = %d; want >= 5", meta.WordCount)
	}
}

func TestExtractPageMeta_Confidence(t *testing.T) {
	content := "---\nconfidence: 0.85\n---\n# Test\n\nContent"
	meta := extractPageMeta("test.md", content)
	if meta.Confidence != 0.85 {
		t.Errorf("Confidence = %f; want 0.85", meta.Confidence)
	}
}

func TestListWikiPages(t *testing.T) {
	tmp := t.TempDir()

	// Create some wiki pages
	pages := map[string]string{
		"index.md":        "# Index\n\nWelcome",
		"log.md":          "# Log\n\n- entry",
		"some-entity.md":  "# Entity\n\nDetails",
		"not-markdown.txt": "plain text",
	}
	for name, content := range pages {
		if err := os.WriteFile(filepath.Join(tmp, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	result, err := listWikiPages(tmp)
	if err != nil {
		t.Fatalf("listWikiPages error: %v", err)
	}

	// Should only include .md files
	if len(result) != 3 {
		t.Errorf("page count = %d; want 3", len(result))
	}

	// Verify sorting: index should come first, then log
	if len(result) >= 2 {
		if result[0].Type != "index" {
			t.Errorf("first page type = %q; want %q", result[0].Type, "index")
		}
		if result[1].Type != "log" {
			t.Errorf("second page type = %q; want %q", result[1].Type, "log")
		}
	}
}

func TestListWikiPages_NestedDir(t *testing.T) {
	tmp := t.TempDir()

	// Create nested structure
	sub := filepath.Join(tmp, "sub")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "root.md"), []byte("# Root"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sub, "nested.md"), []byte("# Nested"), 0o644); err != nil {
		t.Fatal(err)
	}

	result, err := listWikiPages(tmp)
	if err != nil {
		t.Fatalf("listWikiPages error: %v", err)
	}
	if len(result) != 2 {
		t.Errorf("page count = %d; want 2", len(result))
	}
}

func TestListWikiPages_EmptyDir(t *testing.T) {
	tmp := t.TempDir()
	result, err := listWikiPages(tmp)
	if err != nil {
		t.Fatalf("listWikiPages error: %v", err)
	}
	if len(result) != 0 {
		t.Errorf("page count = %d; want 0", len(result))
	}
}

func TestCountMarkdownFiles(t *testing.T) {
	tmp := t.TempDir()
	sub := filepath.Join(tmp, "sub")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}

	// Write 2 .md files and 1 .txt file
	for _, f := range []string{"a.md", "b.md"} {
		if err := os.WriteFile(filepath.Join(tmp, f), []byte("content"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(sub, "c.md"), []byte("nested"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "d.txt"), []byte("text"), 0o644); err != nil {
		t.Fatal(err)
	}

	count := countMarkdownFiles(tmp)
	if count != 3 {
		t.Errorf("countMarkdownFiles = %d; want 3", count)
	}
}

func TestCountMarkdownFiles_Empty(t *testing.T) {
	tmp := t.TempDir()
	count := countMarkdownFiles(tmp)
	if count != 0 {
		t.Errorf("countMarkdownFiles = %d; want 0", count)
	}
}

func TestResolveDir(t *testing.T) {
	tmp := t.TempDir()

	// Non-existent path should return itself
	nonExistent := filepath.Join(tmp, "nonexistent")
	result := resolveDir(nonExistent)
	if result != nonExistent {
		t.Errorf("resolveDir(%q) = %q; want %q", nonExistent, result, nonExistent)
	}

	// Existing path should return the resolved path (possibly same)
	result = resolveDir(tmp)
	if result == "" {
		t.Error("resolveDir returned empty for existing dir")
	}
}

func TestIsAllowedOrigin(t *testing.T) {
	tests := []struct {
		origin string
		want   bool
	}{
		{"", true},
		{"http://localhost:3000", true},
		{"http://localhost:8080", true},
		{"http://localhost", true},
		{"http://127.0.0.1:3000", true},
		{"http://127.0.0.1", true},
		{"http://[::1]:3000", true},
		{"http://[::1]", true},
		{"https://example.com", false},
		{"http://evil.com", false},
		{"http://localhostevil.com", false},
	}
	for _, tt := range tests {
		t.Run(tt.origin, func(t *testing.T) {
			got := isAllowedOrigin(tt.origin)
			if got != tt.want {
				t.Errorf("isAllowedOrigin(%q) = %v; want %v", tt.origin, got, tt.want)
			}
		})
	}
}

func TestCorsJSON(t *testing.T) {
	handler := corsJSON(func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, map[string]string{"status": "ok"})
	})

	t.Run("sets content-type and security headers", func(t *testing.T) {
		req := httptest.NewRequest(http.MethodGet, "/test", nil)
		w := httptest.NewRecorder()
		handler(w, req)

		if ct := w.Header().Get("Content-Type"); ct != "application/json" {
			t.Errorf("Content-Type = %q; want %q", ct, "application/json")
		}
		if xcto := w.Header().Get("X-Content-Type-Options"); xcto != "nosniff" {
			t.Errorf("X-Content-Type-Options = %q; want %q", xcto, "nosniff")
		}
		if xfo := w.Header().Get("X-Frame-Options"); xfo != "DENY" {
			t.Errorf("X-Frame-Options = %q; want %q", xfo, "DENY")
		}
	})

	t.Run("sets CORS for allowed origin", func(t *testing.T) {
		req := httptest.NewRequest(http.MethodGet, "/test", nil)
		req.Header.Set("Origin", "http://localhost:3000")
		w := httptest.NewRecorder()
		handler(w, req)

		acao := w.Header().Get("Access-Control-Allow-Origin")
		if acao != "http://localhost:3000" {
			t.Errorf("ACAO = %q; want %q", acao, "http://localhost:3000")
		}
	})

	t.Run("no CORS for disallowed origin", func(t *testing.T) {
		req := httptest.NewRequest(http.MethodGet, "/test", nil)
		req.Header.Set("Origin", "https://evil.com")
		w := httptest.NewRecorder()
		handler(w, req)

		acao := w.Header().Get("Access-Control-Allow-Origin")
		if acao != "" {
			t.Errorf("ACAO = %q; want empty for disallowed origin", acao)
		}
	})

	t.Run("OPTIONS returns NoContent", func(t *testing.T) {
		req := httptest.NewRequest(http.MethodOptions, "/test", nil)
		w := httptest.NewRecorder()
		handler(w, req)

		if w.Code != http.StatusNoContent {
			t.Errorf("status = %d; want %d", w.Code, http.StatusNoContent)
		}
	})
}

func TestWriteJSON(t *testing.T) {
	w := httptest.NewRecorder()
	data := map[string]string{"key": "value"}
	writeJSON(w, data)

	var result map[string]string
	if err := json.NewDecoder(w.Body).Decode(&result); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if result["key"] != "value" {
		t.Errorf("key = %q; want %q", result["key"], "value")
	}
}

func TestHandlePages_MissingDir(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
	}
}

func TestHandlePages_ValidDir(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "test.md"), []byte("# Test"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages?dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var pages []WikiPageMeta
	if err := json.NewDecoder(w.Body).Decode(&pages); err != nil {
		t.Fatalf("failed to decode pages: %v", err)
	}
	if len(pages) != 1 {
		t.Errorf("page count = %d; want 1", len(pages))
	}
}

func TestHandlePage_MissingParams(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	tests := []struct {
		name  string
		query string
	}{
		{name: "no params", query: ""},
		{name: "only dir", query: "?dir=/tmp"},
		{name: "only path", query: "?path=test.md"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, "/api/wiki/page"+tt.query, nil)
			w := httptest.NewRecorder()
			mux.ServeHTTP(w, req)
			if w.Code != http.StatusBadRequest {
				t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
			}
		})
	}
}

func TestHandlePage_ValidPage(t *testing.T) {
	tmp := t.TempDir()
	content := "# My Page\n\nSome content here"
	if err := os.WriteFile(filepath.Join(tmp, "mypage.md"), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+tmp+"&path=mypage.md", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var page WikiPageContent
	if err := json.NewDecoder(w.Body).Decode(&page); err != nil {
		t.Fatalf("failed to decode page: %v", err)
	}
	if page.Title != "My Page" {
		t.Errorf("Title = %q; want %q", page.Title, "My Page")
	}
	if page.Content != content {
		t.Errorf("Content = %q; want %q", page.Content, content)
	}
}

func TestHandlePage_PathTraversal(t *testing.T) {
	tmp := t.TempDir()
	// Create a file outside the wiki dir
	if err := os.WriteFile(filepath.Join(tmp, "secret.txt"), []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}
	wikiDir := filepath.Join(tmp, "wiki")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+wikiDir+"&path=../secret.txt", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	// Should either be forbidden or not found
	if w.Code == http.StatusOK {
		t.Error("expected path traversal to be blocked")
	}
}

func TestHandlePage_NotFound(t *testing.T) {
	tmp := t.TempDir()

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+tmp+"&path=nonexistent.md", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Errorf("status = %d; want %d", w.Code, http.StatusNotFound)
	}
}

func TestHandleSearch_EmptyParams(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected empty results for empty query, got %d", len(results))
	}
}

func TestHandleSearch_WithResults(t *testing.T) {
	tmp := t.TempDir()

	// Create wiki pages with searchable content
	if err := os.WriteFile(filepath.Join(tmp, "page1.md"), []byte("# Architecture\n\nThis page discusses architecture patterns"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "page2.md"), []byte("# Other Topic\n\nUnrelated content"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=architecture", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) == 0 {
		t.Error("expected at least 1 search result")
	}
}

func TestHandleSessionMessages_MissingID(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/wiki/sessions/messages", corsJSON(h.handleSessionMessages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions/messages", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
	}
}

func TestHandleSessions_MethodNotAllowed(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodPatch, "/api/wiki/sessions", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	// handleSessions uses w.Header() and w.WriteHeader(), so when wrapped
	// with corsJSON, the content-type is set before the handler runs.
	if w.Code != http.StatusMethodNotAllowed {
		t.Errorf("status = %d; want %d", w.Code, http.StatusMethodNotAllowed)
	}
}

func TestHandleSessions_DeleteMissingID(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodDelete, "/api/wiki/sessions", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
	}
}

func TestHandleHubKnowledge_NilHubSvc(t *testing.T) {
	h := &WikiHandler{hubSvc: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/hub-knowledge", corsJSON(h.handleHubKnowledge))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/hub-knowledge", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var items []HubKnowledgeItem
	if err := json.NewDecoder(w.Body).Decode(&items); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(items) != 0 {
		t.Errorf("expected empty list when hubSvc is nil, got %d", len(items))
	}
}

func TestHandleAISearch_MissingBody(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search",
		strings.NewReader("{}"))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error in response for empty query/dir")
	}
}

func TestHandleAISearch_NoAIClient(t *testing.T) {
	h := &WikiHandler{aiClient: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := `{"dir":"/tmp","query":"test query"}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search",
		strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if !strings.Contains(resp.Error, "AI CLI not configured") {
		t.Errorf("expected AI not configured error, got %q", resp.Error)
	}
}

func TestHandleMultiSearch_MissingQuery(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search",
		strings.NewReader(`{"query":""}`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for empty query")
	}
}

func TestHandleMultiSearch_NoAIClient(t *testing.T) {
	h := &WikiHandler{aiClient: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search",
		strings.NewReader(`{"query":"test","wiki_dirs":[]}`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if !strings.Contains(resp.Error, "AI CLI not configured") {
		t.Errorf("expected AI not configured error, got %q", resp.Error)
	}
}

func TestHandleMultiSearch_NoSources(t *testing.T) {
	// Provide a fake aiClient interface by just setting it non-nil
	// but with no valid sources — should return "no valid wiki sources" error
	h := &WikiHandler{aiClient: fakeAIClient{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search",
		strings.NewReader(`{"query":"test","wiki_dirs":[]}`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if !strings.Contains(resp.Error, "no valid wiki sources") {
		t.Errorf("expected 'no valid wiki sources' error, got %q", resp.Error)
	}
}

func TestHandleMultiKeywordSearch_EmptyQuery(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search",
		strings.NewReader(`{"query":""}`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []MultiKeywordResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected empty results, got %d", len(results))
	}
}

func TestUnifiedServerPort(t *testing.T) {
	s := &UnifiedServer{port: 9999, projectName: "test"}
	if s.Port() != 9999 {
		t.Errorf("Port() = %d; want 9999", s.Port())
	}
}

// fakeAIClient satisfies the ai.Client interface for testing.
type fakeAIClient struct{}

func (f fakeAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	return "", nil
}
