package uiserver

import (
	"context"
	"encoding/json"
	"fmt"
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

func TestExtractPageMeta_Links(t *testing.T) {
	content := "# Page\n\nSee [[Other Page]] and [[Another]]."
	meta := extractPageMeta("linked.md", content)
	if len(meta.Links) != 2 {
		t.Errorf("Links count = %d; want 2 (links: %v)", len(meta.Links), meta.Links)
	}
}

func TestExtractPageMeta_NoTags(t *testing.T) {
	content := "# Simple\n\nNo tags here"
	meta := extractPageMeta("simple.md", content)
	if len(meta.Tags) != 0 {
		t.Errorf("Tags count = %d; want 0", len(meta.Tags))
	}
}

func TestExtractPageMeta_EmptyContent(t *testing.T) {
	meta := extractPageMeta("empty.md", "")
	if meta.Title != "empty" {
		t.Errorf("Title = %q; want %q", meta.Title, "empty")
	}
	if meta.WordCount != 0 {
		t.Errorf("WordCount = %d; want 0", meta.WordCount)
	}
	if meta.Type != "entity" {
		t.Errorf("Type = %q; want %q", meta.Type, "entity")
	}
}

func TestExtractPageMeta_InvalidConfidence(t *testing.T) {
	content := "---\nconfidence: notanumber\n---\n# Test"
	meta := extractPageMeta("test.md", content)
	if meta.Confidence != 0 {
		t.Errorf("Confidence = %f; want 0 for invalid value", meta.Confidence)
	}
}

func TestListWikiPages(t *testing.T) {
	tmp := t.TempDir()

	pages := map[string]string{
		"index.md":         "# Index\n\nWelcome",
		"log.md":           "# Log\n\n- entry",
		"some-entity.md":   "# Entity\n\nDetails",
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

	// Verify sorting: all types have rank 1, so sorted alphabetically by path
	// Paths: index.md, log.md, some-entity.md
	if len(result) >= 3 {
		if result[0].Path != "index.md" {
			t.Errorf("first page path = %q; want %q", result[0].Path, "index.md")
		}
		if result[1].Path != "log.md" {
			t.Errorf("second page path = %q; want %q", result[1].Path, "log.md")
		}
		if result[2].Path != "some-entity.md" {
			t.Errorf("third page path = %q; want %q", result[2].Path, "some-entity.md")
		}
	}
}

func TestListWikiPages_SortOrder(t *testing.T) {
	tmp := t.TempDir()

	// Create pages of various types to verify full sort order
	files := map[string]string{
		"entity-z.md":      "# Entity Z",
		"entity-a.md":      "# Entity A",
		"community-hub.md": "# Community Hub",
		"log.md":           "# Log",
		"index.md":         "# Index",
	}
	for name, content := range files {
		if err := os.WriteFile(filepath.Join(tmp, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	result, err := listWikiPages(tmp)
	if err != nil {
		t.Fatal(err)
	}

	if len(result) != 5 {
		t.Fatalf("page count = %d; want 5", len(result))
	}

	// Expected order: community(0), then rank 1 sorted by path: entity-a.md, entity-z.md, index.md, log.md
	expectedTypes := []string{"community", "entity", "entity", "index", "log"}
	for i, et := range expectedTypes {
		if result[i].Type != et {
			t.Errorf("result[%d].Type = %q; want %q", i, result[i].Type, et)
		}
	}

	// Same-rank entities should be sorted by path
	if result[1].Path >= result[2].Path {
		t.Errorf("entities not sorted by path: %q >= %q", result[1].Path, result[2].Path)
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

func TestCountMarkdownFiles_OnlyNonMdFiles(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "readme.txt"), []byte("text"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "config.json"), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	count := countMarkdownFiles(tmp)
	if count != 0 {
		t.Errorf("countMarkdownFiles = %d; want 0 for non-md files only", count)
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

func TestResolveDir_Symlink(t *testing.T) {
	tmp := t.TempDir()
	realDir := filepath.Join(tmp, "real")
	if err := os.MkdirAll(realDir, 0o755); err != nil {
		t.Fatal(err)
	}
	symlink := filepath.Join(tmp, "link")
	if err := os.Symlink(realDir, symlink); err != nil {
		t.Skip("symlinks not supported")
	}

	resolved := resolveDir(symlink)
	if resolved != realDir {
		t.Errorf("resolveDir(symlink) = %q; want %q", resolved, realDir)
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
		{"https://localhost:3000", false},
		{"http://192.168.1.1:3000", false},
		{"ftp://localhost:3000", false},
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

	t.Run("OPTIONS with allowed origin sets CORS header", func(t *testing.T) {
		req := httptest.NewRequest(http.MethodOptions, "/test", nil)
		req.Header.Set("Origin", "http://127.0.0.1:8080")
		w := httptest.NewRecorder()
		handler(w, req)

		if w.Code != http.StatusNoContent {
			t.Errorf("status = %d; want %d", w.Code, http.StatusNoContent)
		}
		acao := w.Header().Get("Access-Control-Allow-Origin")
		if acao != "http://127.0.0.1:8080" {
			t.Errorf("ACAO = %q; want %q", acao, "http://127.0.0.1:8080")
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

func TestWriteJSON_Array(t *testing.T) {
	w := httptest.NewRecorder()
	data := []int{1, 2, 3}
	writeJSON(w, data)

	var result []int
	if err := json.NewDecoder(w.Body).Decode(&result); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(result) != 3 {
		t.Errorf("len = %d; want 3", len(result))
	}
}

func TestWriteJSON_EmptySlice(t *testing.T) {
	w := httptest.NewRecorder()
	writeJSON(w, []string{})

	var result []string
	if err := json.NewDecoder(w.Body).Decode(&result); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(result) != 0 {
		t.Errorf("len = %d; want 0", len(result))
	}
}

func TestWriteJSON_NilSlice(t *testing.T) {
	w := httptest.NewRecorder()
	var data []string
	writeJSON(w, data)

	body := strings.TrimSpace(w.Body.String())
	if body != "null" {
		t.Errorf("body = %q; want %q", body, "null")
	}
}

func TestHandleModules(t *testing.T) {
	// Test with a temp directory that has no wiki content — should return empty module list
	tmp := t.TempDir()
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
		t.Fatalf("failed to decode modules: %v", err)
	}
	// modules list should be a valid JSON array (possibly empty)
	t.Logf("got %d modules", len(modules))
}

func TestHandleModules_EmptyProjectDir(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/modules", corsJSON(h.handleModules))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/modules", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var modules []WikiModule
	if err := json.NewDecoder(w.Body).Decode(&modules); err != nil {
		t.Fatalf("failed to decode modules: %v", err)
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

func TestHandlePages_NonexistentDir(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages?dir=/nonexistent-dir-"+fmt.Sprintf("%d", os.Getpid()), nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d; want %d", w.Code, http.StatusBadRequest)
	}
}

func TestHandlePages_FileNotDir(t *testing.T) {
	tmp := t.TempDir()
	f := filepath.Join(tmp, "afile.txt")
	if err := os.WriteFile(f, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages?dir="+f, nil)
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

func TestHandlePage_ReturnsMetaAndContent(t *testing.T) {
	tmp := t.TempDir()
	content := "---\ntags: [test, wiki]\nconfidence: 0.9\nsource: auto\n---\n# Rich Page\n\nSee [[Other Page]].\n\nMore content here with words."
	if err := os.WriteFile(filepath.Join(tmp, "rich.md"), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/page?dir="+tmp+"&path=rich.md", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var page WikiPageContent
	if err := json.NewDecoder(w.Body).Decode(&page); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if page.Title != "Rich Page" {
		t.Errorf("Title = %q; want %q", page.Title, "Rich Page")
	}
	if page.Source != "auto" {
		t.Errorf("Source = %q; want %q", page.Source, "auto")
	}
	if page.Confidence != 0.9 {
		t.Errorf("Confidence = %f; want 0.9", page.Confidence)
	}
	if len(page.Tags) != 2 {
		t.Errorf("Tags = %v; want 2 tags", page.Tags)
	}
	if page.Content != content {
		t.Errorf("Content mismatch")
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

func TestHandleSearch_MissingQuery(t *testing.T) {
	tmp := t.TempDir()
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	// dir only, no query
	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected empty results for missing query, got %d", len(results))
	}
}

func TestHandleSearch_MissingDir(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	// query only, no dir
	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?q=test", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected empty results for missing dir, got %d", len(results))
	}
}

func TestHandleSearch_WithResults(t *testing.T) {
	tmp := t.TempDir()

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

func TestHandleSearch_MultipleResults(t *testing.T) {
	tmp := t.TempDir()

	// Create pages that both match "golang"
	if err := os.WriteFile(filepath.Join(tmp, "page1.md"), []byte("# Go Programming\n\nLearn golang golang golang here"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "page2.md"), []byte("# More Go\n\nMore about golang"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=golang", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) < 2 {
		t.Errorf("expected at least 2 results, got %d", len(results))
	}
	// Results should be sorted by score descending
	if len(results) >= 2 && results[0].Score < results[1].Score {
		t.Errorf("results not sorted by score: %d < %d", results[0].Score, results[1].Score)
	}
}

func TestHandleSearch_NoMatch(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "page.md"), []byte("# Hello\n\nWorld"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=xyznotfoundxyz", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(results) != 0 {
		t.Errorf("expected 0 results for non-matching query, got %d", len(results))
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

func TestHandleAISearch_InvalidJSON(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search",
		strings.NewReader("not-json"))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for invalid JSON body")
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

func TestHandleAISearch_WithFakeAI(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "index.md"), []byte("# Index\n\nMain wiki index"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "topic.md"), []byte("---\ntags: [arch]\n---\n# Topic\n\nSome architecture info"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Use a fakeAIClient that returns valid JSON
	h := &WikiHandler{aiClient: fakeAIClient{response: `{"answer":"This is about architecture.","results":[{"path":"topic.md","title":"Topic","relevance":"relevant","score":90}]}`}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"architecture"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Answer == "" {
		t.Error("expected non-empty answer")
	}
	if resp.SessionID == "" {
		t.Error("expected non-empty session ID")
	}
	if len(resp.Results) != 1 {
		t.Errorf("expected 1 result, got %d", len(resp.Results))
	}
	if resp.Error != "" {
		t.Errorf("unexpected error: %s", resp.Error)
	}
}

func TestHandleAISearch_AIReturnsFencedJSON(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "doc.md"), []byte("# Doc\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	// AI returns response wrapped in code fences
	fenced := "```json\n{\"answer\":\"fenced answer\",\"results\":[]}\n```"
	h := &WikiHandler{aiClient: fakeAIClient{response: fenced}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"test"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Answer != "fenced answer" {
		t.Errorf("Answer = %q; want %q", resp.Answer, "fenced answer")
	}
}

func TestHandleAISearch_AIReturnsPlainText(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "doc.md"), []byte("# Doc\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	// AI returns non-JSON response
	h := &WikiHandler{aiClient: fakeAIClient{response: "This is a plain text answer, not JSON"}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"test"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should fall back to putting the raw response as the answer
	if resp.Answer == "" {
		t.Error("expected non-empty answer for plain text AI response")
	}
}

func TestHandleAISearch_AIError(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "doc.md"), []byte("# Doc\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{aiClient: fakeAIClient{err: fmt.Errorf("ai network error")}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"test"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !strings.Contains(resp.Error, "AI query failed") {
		t.Errorf("expected AI query failed error, got %q", resp.Error)
	}
}

func TestHandleAISearch_FrontmatterStripping(t *testing.T) {
	tmp := t.TempDir()
	// Create a page with frontmatter that should be stripped from catalog
	longFM := "---\ntitle: Test\ntags: [a, b]\n---\n# Test Page\n\nActual content here that is pretty long " + strings.Repeat("word ", 100)
	if err := os.WriteFile(filepath.Join(tmp, "test.md"), []byte(longFM), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{aiClient: fakeAIClient{response: `{"answer":"ok","results":[]}`}}
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

func TestHandleAISearch_ValidatesResultPaths(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "real.md"), []byte("# Real\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	// AI returns results that include a non-existent page path
	h := &WikiHandler{aiClient: fakeAIClient{response: `{"answer":"test","results":[{"path":"real.md","title":"","relevance":"yes","score":90},{"path":"fake.md","title":"Fake","relevance":"no","score":50}]}`}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	body := fmt.Sprintf(`{"dir":%q,"query":"test"}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/ai-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp AISearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should filter out fake.md and fill in title from catalog for real.md
	if len(resp.Results) != 1 {
		t.Errorf("expected 1 validated result, got %d", len(resp.Results))
	}
	if len(resp.Results) == 1 && resp.Results[0].Title == "" {
		t.Error("expected title to be filled from catalog")
	}
}

func TestDiscoverModules_NoProjectDir(t *testing.T) {
	modules := discoverModules("")
	// Should not panic
	t.Logf("got %d modules with empty projectDir", len(modules))
}

func TestRegisterAPIRoutes(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	h.RegisterAPIRoutes(mux)

	// Verify that the routes exist by making test requests
	// Each route should return something other than 404 (which means unregistered).
	// Some routes return 400/200 depending on params, but NOT 404 because they ARE registered.
	endpoints := []struct {
		method  string
		path    string
		wantNot int
	}{
		{http.MethodGet, "/api/wiki/modules", http.StatusNotFound},
		{http.MethodGet, "/api/wiki/pages", http.StatusNotFound},  // returns 400 (dir missing)
		{http.MethodGet, "/api/wiki/page", http.StatusNotFound},   // returns 400 (dir+path missing)
		{http.MethodGet, "/api/wiki/search", http.StatusNotFound}, // returns 200 (empty results)
	}

	for _, ep := range endpoints {
		req := httptest.NewRequest(ep.method, ep.path, nil)
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, req)
		if w.Code == ep.wantNot {
			t.Errorf("route %s %s not registered (got %d)", ep.method, ep.path, w.Code)
		}
	}
}

func TestUnifiedServerPort(t *testing.T) {
	s := &UnifiedServer{port: 9999, projectName: "test"}
	if s.Port() != 9999 {
		t.Errorf("Port() = %d; want 9999", s.Port())
	}
}

func TestLoadProjectIDNames(t *testing.T) {
	// This function reads from brand.GlobalDir() + hub.registry.json
	// Just verify it doesn't panic and returns a map
	names := loadProjectIDNames()
	if names == nil {
		t.Error("loadProjectIDNames returned nil")
	}
}

// Additional tests for remaining coverage gaps

// The scanDir/discoverContexts tests that used to live here were deleted with the
// functions themselves: context membership is a per-project record now, not a walk of
// a directory that no longer holds anything. What replaced them is in
// wiki_modules_test.go, which asserts against the resolvers instead of a tree.

func TestHandleSearch_FallbackSearch(t *testing.T) {
	tmp := t.TempDir()

	// Create pages — BM25 may or may not find them depending on content structure.
	// The fallback search (lines 184-218) kicks in when BM25 returns no results.
	// Use a very specific query that BM25 might miss but raw text search will find.
	if err := os.WriteFile(filepath.Join(tmp, "page1.md"), []byte("# Title\n\nThis is xyz123uniquetoken content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "page2.md"), []byte("# Other\n\nDifferent xyz123uniquetoken text and more xyz123uniquetoken"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=xyz123uniquetoken", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Should find at least 1 result through either BM25 or fallback
	if len(results) == 0 {
		t.Error("expected search to find results via BM25 or fallback text search")
	}
}

func TestHandleSearch_LongContent(t *testing.T) {
	tmp := t.TempDir()

	// Create a page with very long content to exercise snippet extraction boundary cases
	longContent := "# Long Page\n\n" + strings.Repeat("word ", 500) + "uniquetoken" + strings.Repeat(" filler", 500)
	if err := os.WriteFile(filepath.Join(tmp, "long.md"), []byte(longContent), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=uniquetoken", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(results) > 0 && results[0].Snippet == "" {
		t.Error("expected non-empty snippet")
	}
}

func TestLoadProjectIDNames_WithRegistryFile(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("HOME", tmp)

	globalDir := filepath.Join(tmp, ".config", "graphit")
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}
	registry := `{"projects":{"proj-id-1":{"name":"My Project"},"proj-id-2":{"name":"Another"}}}`
	registryPath := filepath.Join(globalDir, "hub.registry.json")
	if err := os.WriteFile(registryPath, []byte(registry), 0o644); err != nil {
		t.Fatal(err)
	}

	// The function reads from brand.GlobalDir() which uses XDG_CONFIG_HOME or HOME
	names := loadProjectIDNames()
	// names may or may not contain the entries depending on brand.GlobalDir() implementation
	// Just verify it doesn't crash and returns a map
	if names == nil {
		t.Error("expected non-nil map")
	}
}

// The label tests moved to wiki_modules_test.go, where the wiki they need exists in an
// isolated global store rather than in a project subdirectory that no longer holds one.

// fakeAIClient satisfies the ai.Client interface for testing.
type fakeAIClient struct {
	response string
	err      error
}

func (f fakeAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	if f.err != nil {
		return "", f.err
	}
	return f.response, nil
}
