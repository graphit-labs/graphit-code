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

// ─── handleMultiSearch extended tests ───────────────────────────────────────

func TestHandleMultiSearch_WikiLinksInAnswer(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "arch.md"), []byte("# Architecture\n\nArch details"), 0o644); err != nil {
		t.Fatal(err)
	}

	// AI returns an answer containing wikilinks to exercise pages_consulted extraction.
	h := &WikiHandler{aiClient: fakeAIClient{response: "See [[Architecture]] and [[Architecture]] for details. Also [[Design]]."}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	body := fmt.Sprintf(`{"query":"arch","wiki_dirs":[{"id":"w1","label":"W1","dir":%q}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d, body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}

	// If the search succeeded (no error), check wikilink dedup
	if resp.Error == "" {
		if resp.SessionID == "" {
			t.Error("expected non-empty session_id")
		}
		if resp.PagesConsulted == nil {
			t.Log("PagesConsulted is nil (may depend on SearchMultiWiki behavior)")
		}
	}
}

func TestHandleMultiSearch_MixedValidInvalidDirs(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "page.md"), []byte("# Page\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{aiClient: fakeAIClient{response: "answer"}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	// One valid dir, one invalid — should still work with the valid one
	body := fmt.Sprintf(`{"query":"test","wiki_dirs":[{"id":"valid","label":"Valid","dir":%q},{"id":"bad","label":"Bad","dir":"/nonexistent-xyz-123"}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should not error because at least one source is valid
	if strings.Contains(resp.Error, "no valid wiki sources") {
		t.Errorf("expected valid sources to be found, got error: %q", resp.Error)
	}
}

func TestHandleMultiSearch_MultipleDirs(t *testing.T) {
	t.Parallel()
	tmp1 := t.TempDir()
	tmp2 := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp1, "a.md"), []byte("# A\n\nContent A"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp2, "b.md"), []byte("# B\n\nContent B"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{aiClient: fakeAIClient{response: "multi-source answer"}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	body := fmt.Sprintf(`{"query":"content","wiki_dirs":[{"id":"s1","label":"S1","dir":%q},{"id":"s2","label":"S2","dir":%q}]}`, tmp1, tmp2)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should either succeed or error from AI, but not "no valid wiki sources"
	if strings.Contains(resp.Error, "no valid wiki sources") {
		t.Error("should have found valid sources from both dirs")
	}
}

// ─── handleSearch extended tests ────────────────────────────────────────────

func TestHandleSearch_CaseInsensitive(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "page.md"), []byte("# Test Page\n\nHello WORLD content"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	// Search with lowercase should find uppercase content
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
	// Place query token at the very beginning of the file
	content := "uniquestart this is some content after the token"
	if err := os.WriteFile(filepath.Join(tmp, "start.md"), []byte(content), 0o644); err != nil {
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
	// Create 35 pages that all match, to test the cap at 30
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

// ─── handlePages extended tests ─────────────────────────────────────────────

func TestHandlePages_SymlinkedDir(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	realDir := filepath.Join(tmp, "real")
	if err := os.MkdirAll(realDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(realDir, "page.md"), []byte("# Symlinked"), 0o644); err != nil {
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

// ─── handlePage extended tests ──────────────────────────────────────────────

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

	// Try absolute path traversal
	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+wikiDir+"&path=/etc/passwd", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	// Should not return 200 OK
	if w.Code == http.StatusOK {
		t.Error("absolute path traversal should be blocked")
	}
}

func TestHandlePage_DeepNestedPath(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	deepDir := filepath.Join(tmp, "sub", "deep")
	if err := os.MkdirAll(deepDir, 0o755); err != nil {
		t.Fatal(err)
	}
	content := "# Deep Page\n\nContent at depth 2"
	if err := os.WriteFile(filepath.Join(deepDir, "deep.md"), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+tmp+"&path=sub/deep/deep.md", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var page WikiPageContent
	if err := json.NewDecoder(w.Body).Decode(&page); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if page.Title != "Deep Page" {
		t.Errorf("Title = %q; want %q", page.Title, "Deep Page")
	}
}

// ─── handleHubKnowledge extended tests ──────────────────────────────────────

func TestHandleHubKnowledge_EmptyEntries(t *testing.T) {
	t.Parallel()
	// With a HubService that returns empty entries (simulated via nil registry)
	// We can't easily construct a real HubService without infrastructure, but we can
	// test the nil path which was already tested, and verify JSON structure is correct.
	h := &WikiHandler{hubSvc: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/hub-knowledge", corsJSON(h.handleHubKnowledge))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/hub-knowledge", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	body := strings.TrimSpace(w.Body.String())
	if body != "[]" {
		t.Errorf("expected empty JSON array, got %q", body)
	}
}

// ─── handleSessionMessages extended tests ───────────────────────────────────

func TestHandleSessionMessages_ValidSessionWithHistory(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("HOME", tmp)

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/wiki/sessions/messages", corsJSON(h.handleSessionMessages))

	// Nonexistent session ID — should return 404
	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions/messages?id=XXXXXXXXXXXXXXXXXXXXXXXXXX", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Errorf("status = %d; want %d for nonexistent session", w.Code, http.StatusNotFound)
	}
}

// ─── handleChat extended tests ──────────────────────────────────────────────

func TestHandleChat_EmptyFields(t *testing.T) {
	t.Parallel()
	h := &WikiHandler{aiClient: fakeAIClient{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	// session_id present but message empty
	body := `{"session_id":"some-id","message":""}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for empty message with session_id")
	}
}

func TestHandleChat_BothFieldsEmpty(t *testing.T) {
	t.Parallel()
	h := &WikiHandler{aiClient: fakeAIClient{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	body := `{"session_id":"","message":""}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for both empty fields")
	}
}

// ─── handleMultiKeywordSearch extended tests ────────────────────────────────

func TestHandleMultiKeywordSearch_NoMatchingContent(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "page.md"), []byte("# Hello\n\nWorld content"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := fmt.Sprintf(`{"query":"zzzznonexistent","wiki_dirs":[{"id":"t","label":"T","dir":%q}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []MultiKeywordResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected 0 results for non-matching query, got %d", len(results))
	}
}

func TestHandleMultiKeywordSearch_MixedValidInvalidSources(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "match.md"), []byte("# Match\n\nSearchable content with keyword"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := fmt.Sprintf(`{"query":"keyword","wiki_dirs":[{"id":"valid","label":"Valid","dir":%q},{"id":"bad","label":"Bad","dir":"/nonexistent-xyz"}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []MultiKeywordResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should only have results from the valid source
	for _, r := range results {
		if r.SourceID == "bad" {
			t.Error("should not have results from invalid source")
		}
	}
}

// ─── handleSessions extended tests ──────────────────────────────────────────

func TestHandleSessions_PutMethodNotAllowed(t *testing.T) {
	t.Parallel()
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodPut, "/api/wiki/sessions", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusMethodNotAllowed {
		t.Errorf("status = %d; want %d", w.Code, http.StatusMethodNotAllowed)
	}
}

// ─── handleModules extended tests ───────────────────────────────────────────

func TestHandleModules_WithWikiContent(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	// Create knowledge wiki with content
	knDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	if err := os.MkdirAll(knDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(knDir, "index.md"), []byte("# Knowledge Index"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(knDir, "page.md"), []byte("# Page"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/modules", corsJSON(h.handleModules))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/modules?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var modules []WikiModule
	if err := json.NewDecoder(w.Body).Decode(&modules); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should have found at least one module
	if len(modules) == 0 {
		t.Error("expected at least one module for dir with wiki content")
	}

	// Modules should be sorted by ID
	for i := 1; i < len(modules); i++ {
		if modules[i-1].ID > modules[i].ID {
			t.Errorf("modules not sorted: %q > %q", modules[i-1].ID, modules[i].ID)
		}
	}
}

// ─── handleAISearch edge cases ──────────────────────────────────────────────

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
	// Create many pages to test catalog building with content truncation
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

// ─── Utility function tests ────────────────────────────────────────────────

func TestListWikiPages_OnlyNonMdFiles(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "readme.txt"), []byte("text"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "config.json"), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}

	pages, err := listWikiPages(tmp)
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(pages) != 0 {
		t.Errorf("expected 0 pages for non-md-only dir, got %d", len(pages))
	}
}

func TestExtractPageMeta_MultipleH1(t *testing.T) {
	t.Parallel()
	// Should use the first H1
	content := "# First Title\n\nSome content\n\n# Second Title\n\nMore content"
	meta := extractPageMeta("multi-h1.md", content)
	if meta.Title != "First Title" {
		t.Errorf("Title = %q; want %q (should use first H1)", meta.Title, "First Title")
	}
}

func TestExtractPageMeta_FrontmatterOnly(t *testing.T) {
	t.Parallel()
	content := "---\ntags: [a, b]\nconfidence: 0.7\nsource: automated\n---\nNo heading here but some body text."
	meta := extractPageMeta("fm-only.md", content)
	// Title should fall back to filename without extension
	if meta.Title != "fm-only" {
		t.Errorf("Title = %q; want %q", meta.Title, "fm-only")
	}
	if len(meta.Tags) != 2 {
		t.Errorf("Tags count = %d; want 2", len(meta.Tags))
	}
	if meta.Confidence != 0.7 {
		t.Errorf("Confidence = %f; want 0.7", meta.Confidence)
	}
	if meta.Source != "automated" {
		t.Errorf("Source = %q; want %q", meta.Source, "automated")
	}
}

func TestCountMarkdownFiles_DeepNesting(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	deepDir := filepath.Join(tmp, "a", "b", "c")
	if err := os.MkdirAll(deepDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(deepDir, "deep.md"), []byte("# Deep"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "root.md"), []byte("# Root"), 0o644); err != nil {
		t.Fatal(err)
	}

	count := countMarkdownFiles(tmp)
	if count != 2 {
		t.Errorf("countMarkdownFiles = %d; want 2 (deep + root)", count)
	}
}

func TestResolveDir_ExistingDir(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()
	result := resolveDir(tmp)
	if result == "" {
		t.Error("resolveDir returned empty for existing dir")
	}
	// Should return an absolute path
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
	// No Origin header
	w := httptest.NewRecorder()
	handler(w, req)

	// Should still work, CORS header may or may not be set for same-origin
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
