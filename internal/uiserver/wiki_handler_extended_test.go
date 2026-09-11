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

func TestHandleSearch_CaseInsensitive(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := indexPage(t, tmp, "page.md", "# Test Page\n\nHello WORLD content"); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=world", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(results) == 0 {
		t.Error("expected case-insensitive search to find result")
	}
}

func TestHandleSearch_SnippetBoundary(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	content := "uniquestart this is some content after the token"
	if err := indexPage(t, tmp, "start.md", content); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=uniquestart", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(results) == 0 {
		t.Error("expected result for token at start of content")
	}
	if len(results) > 0 && results[0].Snippet == "" {
		t.Error("expected non-empty snippet")
	}
}

func TestHandleSearch_ResultsCapped(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	for i := 0; i < 35; i++ {
		name := fmt.Sprintf("page%03d.md", i)
		content := fmt.Sprintf("# Page %d\n\nThis is about xcaptest topic", i)
		if err := os.WriteFile(filepath.Join(tmp, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=xcaptest", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(results) > 30 {
		t.Errorf("expected at most 30 results (cap), got %d", len(results))
	}
}

func TestHandlePages_SymlinkedDir(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	realDir := filepath.Join(tmp, "real")
	if err := os.MkdirAll(realDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := indexPage(t, realDir, "page.md", "# Symlinked"); err != nil {
		t.Fatal(err)
	}
	linkDir := filepath.Join(tmp, "link")
	if err := os.Symlink(realDir, linkDir); err != nil {
		t.Skip("symlinks not supported")
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages?dir="+linkDir, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var pages []WikiPageMeta
	if err := json.NewDecoder(w.Body).Decode(&pages); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(pages) != 1 {
		t.Errorf("expected 1 page via symlink, got %d", len(pages))
	}
}

func TestHandlePages_EmptyDir(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages?dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var pages []WikiPageMeta
	if err := json.NewDecoder(w.Body).Decode(&pages); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(pages) != 0 {
		t.Errorf("expected 0 pages in empty dir, got %d", len(pages))
	}
}

func TestHandlePage_AbsolutePathTraversal(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	wikiDir := filepath.Join(tmp, "wiki")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "secret.txt"), []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+wikiDir+"&path=/etc/passwd", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code == http.StatusOK {
		t.Error("absolute path traversal should be blocked")
	}
}

func TestHandlePage_APathIsNotASlug(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	content := "# Deep Page\n\nContent at depth 2"
	if err := indexPage(t, tmp, "deep.md", content); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	get := func(path string) *httptest.ResponseRecorder {
		req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+tmp+"&path="+path, nil)
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, req)
		return w
	}

	if w := get("sub/deep/deep.md"); w.Code != http.StatusNotFound {
		t.Errorf("nested path status = %d; want %d", w.Code, http.StatusNotFound)
	}

	w := get("deep.md")
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}
	var page WikiPageContent
	if err := json.NewDecoder(w.Body).Decode(&page); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if page.Title != "Deep Page" {
		t.Errorf("Title = %q; want %q", page.Title, "Deep Page")
	}
}

func TestHandleAISearch_EmptyDir(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	h := &WikiHandler{aiClient: fakeAIClient{response: `{"answer":"no pages","results":[]}`}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"test"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}
}

func TestHandleAISearch_ManyPages(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	for i := 0; i < 5; i++ {
		name := fmt.Sprintf("page%d.md", i)
		content := fmt.Sprintf("---\ntags: [test]\n---\n# Page %d\n\n%s", i, strings.Repeat("word ", 100))
		if err := os.WriteFile(filepath.Join(tmp, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	h := &WikiHandler{aiClient: fakeAIClient{response: `{"answer":"aggregated","results":[]}`}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"word"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Answer != "aggregated" {
		t.Errorf("Answer = %q; want %q", resp.Answer, "aggregated")
	}
}

func TestResolveDir_ExistingDir(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	result := resolveDir(tmp)
	if result == "" {
		t.Error("resolveDir returned empty for existing dir")
	}
	if !filepath.IsAbs(result) {
		t.Errorf("resolveDir returned non-absolute path: %q", result)
	}
}

func TestCorsJSON_NoOrigin(t *testing.T) {
	t.Parallel()
	handler := corsJSON(func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, map[string]string{"ok": "true"})
	})

	req := httptest.NewRequest(http.MethodGet, "/test", nil)
	w := httptest.NewRecorder()
	handler(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}
}

func TestWriteJSON_NestedStruct(t *testing.T) {
	t.Parallel()
	w := httptest.NewRecorder()
	data := WikiPageContent{
		WikiPageMeta: WikiPageMeta{
			Path:  "test.md",
			Title: "Test",
			Type:  "entity",
		},
		Content: "# Test\n\nBody",
	}
	writeJSON(w, data)

	var result WikiPageContent
	if err := json.NewDecoder(w.Body).Decode(&result); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if result.Title != "Test" {
		t.Errorf("Title = %q; want %q", result.Title, "Test")
	}
	if result.Content != "# Test\n\nBody" {
		t.Errorf("Content mismatch")
	}
}
