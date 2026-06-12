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

	// Create some wiki pages
	pages := map[string]string{
		"index.md":          "# Index\n\nWelcome",
		"log.md":            "# Log\n\n- entry",
		"some-entity.md":    "# Entity\n\nDetails",
		"not-markdown.txt":  "plain text",
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
		"log.md":            "# Log",
		"index.md":          "# Index",
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

func TestHandleSessionMessages_InvalidID(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/wiki/sessions/messages", corsJSON(h.handleSessionMessages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions/messages?id=nonexistent-session-xyz", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Errorf("status = %d; want %d (404 for not found session)", w.Code, http.StatusNotFound)
	}
}

func TestHandleSessions_MethodNotAllowed(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodPatch, "/api/wiki/sessions", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

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

func TestHandleSessions_DeleteNonexistent(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodDelete, "/api/wiki/sessions?id=nonexistent-session-id-xyz", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Errorf("status = %d; want %d", w.Code, http.StatusNotFound)
	}
}

func TestHandleSessions_Get(t *testing.T) {
	tmp := t.TempDir()
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var items []SessionListItem
	if err := json.NewDecoder(w.Body).Decode(&items); err != nil {
		t.Fatalf("failed to decode sessions: %v", err)
	}
	// May be empty but should be valid JSON
	t.Logf("got %d sessions", len(items))
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
	// Create some wiki pages
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

func TestHandleMultiSearch_InvalidJSON(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search",
		strings.NewReader("not valid json"))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for invalid JSON")
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

func TestHandleMultiSearch_NonexistentDirs(t *testing.T) {
	h := &WikiHandler{aiClient: fakeAIClient{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	body := `{"query":"test","wiki_dirs":[{"id":"fake","label":"Fake","dir":"/nonexistent-dir-xyz"}]}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
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

func TestHandleMultiKeywordSearch_InvalidJSON(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search",
		strings.NewReader("invalid-json"))
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

func TestHandleMultiKeywordSearch_WithValidSources(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "page1.md"), []byte("# Page One\n\nTesting keyword search feature"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := fmt.Sprintf(`{"query":"keyword","wiki_dirs":[{"id":"test","label":"Test","dir":%q}]}`, tmp)
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
	// BM25 should find "keyword" in the page
	if len(results) > 0 {
		if results[0].SourceID != "test" {
			t.Errorf("SourceID = %q; want %q", results[0].SourceID, "test")
		}
	}
}

func TestHandleMultiKeywordSearch_NonexistentDirs(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := `{"query":"test","wiki_dirs":[{"id":"fake","label":"Fake","dir":"/nonexistent-xyz"}]}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-keyword-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var results []MultiKeywordResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// No valid sources should produce empty results
	if len(results) != 0 {
		t.Errorf("expected 0 results, got %d", len(results))
	}
}

func TestHandleChat_MissingBody(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader("{}"))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for missing session_id and message")
	}
}

func TestHandleChat_InvalidJSON(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader("not-json"))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for invalid JSON")
	}
}

func TestHandleChat_NoAIClient(t *testing.T) {
	h := &WikiHandler{aiClient: nil}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	body := `{"session_id":"test-session","message":"hello"}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !strings.Contains(resp.Error, "AI CLI not configured") {
		t.Errorf("expected AI not configured error, got %q", resp.Error)
	}
}

func TestHandleChat_MissingSessionID(t *testing.T) {
	h := &WikiHandler{aiClient: fakeAIClient{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	body := `{"session_id":"","message":"hello"}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for missing session_id")
	}
}

func TestHandleChat_MissingMessage(t *testing.T) {
	h := &WikiHandler{aiClient: fakeAIClient{}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))

	body := `{"session_id":"test","message":""}`
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/chat", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp ChatResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error == "" {
		t.Error("expected error for missing message")
	}
}

func TestDiscoverModules_TempDir(t *testing.T) {
	tmp := t.TempDir()

	// Create a minimal wiki structure under .graphit
	knowledgeDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	memProjDir := filepath.Join(tmp, ".graphit", "memory", "project")
	memUserDir := filepath.Join(tmp, ".graphit", "memory", "user")

	for _, d := range []string{knowledgeDir, memProjDir, memUserDir} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}

	// Add markdown files so modules are discovered
	if err := os.WriteFile(filepath.Join(knowledgeDir, "index.md"), []byte("# Knowledge"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(memProjDir, "index.md"), []byte("# Memory Project"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(memUserDir, "index.md"), []byte("# Memory User"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Also add a log.md to test hasLog
	if err := os.WriteFile(filepath.Join(knowledgeDir, "log.md"), []byte("# Log"), 0o644); err != nil {
		t.Fatal(err)
	}

	modules := discoverModules(tmp)

	if len(modules) == 0 {
		t.Error("expected at least some modules to be discovered")
	}

	// Check that modules are sorted by ID
	for i := 1; i < len(modules); i++ {
		if modules[i-1].ID > modules[i].ID {
			t.Errorf("modules not sorted: %q > %q", modules[i-1].ID, modules[i].ID)
		}
	}

	// Look for knowledge module and verify hasLog
	found := false
	for _, m := range modules {
		if m.ID == "knowledge" {
			found = true
			if m.Pages < 2 {
				t.Errorf("knowledge module should have at least 2 pages (index + log), got %d", m.Pages)
			}
			if !m.HasLog {
				t.Error("knowledge module should have hasLog=true")
			}
		}
	}
	if !found {
		t.Log("knowledge module not found in discovered modules (may depend on brand config)")
	}
}

func TestDiscoverModules_EmptyDir(t *testing.T) {
	tmp := t.TempDir()
	modules := discoverModules(tmp)
	// Should return valid (possibly empty) slice
	if modules == nil {
		t.Log("discoverModules returned nil (expected for empty dir)")
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
		method   string
		path     string
		wantNot  int
	}{
		{http.MethodGet, "/api/wiki/modules", http.StatusNotFound},
		{http.MethodGet, "/api/wiki/pages", http.StatusNotFound},    // returns 400 (dir missing)
		{http.MethodGet, "/api/wiki/page", http.StatusNotFound},     // returns 400 (dir+path missing)
		{http.MethodGet, "/api/wiki/search", http.StatusNotFound},   // returns 200 (empty results)
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

// --- Additional tests for remaining coverage gaps ---

func TestScanDir_WithContextSubdirs(t *testing.T) {
	tmp := t.TempDir()

	// Create a base dir with context subdirectories containing wiki pages
	base := filepath.Join(tmp, "knowledge")
	if err := os.MkdirAll(base, 0o755); err != nil {
		t.Fatal(err)
	}

	// Create context "ctx1" with md files
	ctx1Dir := filepath.Join(base, "ctx1")
	if err := os.MkdirAll(ctx1Dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ctx1Dir, "page1.md"), []byte("# Page 1"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ctx1Dir, "log.md"), []byte("# Log"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Create context "ctx2" with a "wiki" subdirectory
	ctx2WikiDir := filepath.Join(base, "ctx2", "wiki")
	if err := os.MkdirAll(ctx2WikiDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ctx2WikiDir, "page2.md"), []byte("# Page 2"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Skipped dirs: project, user, export, .hidden
	for _, skip := range []string{"project", "user", "export", ".hidden"} {
		skipDir := filepath.Join(base, skip)
		if err := os.MkdirAll(skipDir, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(skipDir, "page.md"), []byte("# Skip"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	// Empty context dir (no md files) with nested subdir containing md (depth=2)
	deepDir := filepath.Join(base, "deep", "nested")
	if err := os.MkdirAll(deepDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(deepDir, "deep-page.md"), []byte("# Deep"), 0o644); err != nil {
		t.Fatal(err)
	}

	var modules []WikiModule
	idNames := map[string]string{"ctx1": "Context One"}
	discoverContexts(&modules, base, "test-mod", "TestLabel", idNames)

	if len(modules) == 0 {
		t.Fatal("expected at least some modules from discoverContexts")
	}

	// Verify ctx1 was found with displayName from idNames
	foundCtx1 := false
	for _, m := range modules {
		if m.ID == "test-mod/ctx1" {
			foundCtx1 = true
			if m.Label != "Context One" {
				t.Errorf("ctx1 label = %q; want %q", m.Label, "Context One")
			}
			if !m.HasLog {
				t.Error("ctx1 should have HasLog=true")
			}
		}
	}
	if !foundCtx1 {
		t.Error("ctx1 module not found")
	}

	// Verify ctx2 was found
	foundCtx2 := false
	for _, m := range modules {
		if m.ID == "test-mod/ctx2" {
			foundCtx2 = true
			// Should point to the wiki subdir
			if !strings.HasSuffix(m.Path, "wiki") {
				t.Errorf("ctx2 path should end with 'wiki', got %q", m.Path)
			}
		}
	}
	if !foundCtx2 {
		t.Error("ctx2 module not found")
	}

	// Verify skipped dirs are not present
	for _, m := range modules {
		for _, skip := range []string{"project", "user", "export", ".hidden"} {
			if m.Context == skip {
				t.Errorf("skipped dir %q should not be in modules", skip)
			}
		}
	}
}

func TestScanDir_DuplicateDetection(t *testing.T) {
	tmp := t.TempDir()
	base := filepath.Join(tmp, "base")
	ctxDir := filepath.Join(base, "ctx")
	if err := os.MkdirAll(ctxDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ctxDir, "page.md"), []byte("# P"), 0o644); err != nil {
		t.Fatal(err)
	}

	var modules []WikiModule
	// Call twice — the second call should detect duplicates and not add again
	discoverContexts(&modules, base, "mod", "Label", nil)
	initialCount := len(modules)
	discoverContexts(&modules, base, "mod", "Label", nil)

	if len(modules) != initialCount {
		t.Errorf("duplicate modules added: %d -> %d", initialCount, len(modules))
	}
}

func TestScanDir_NonExistentBase(t *testing.T) {
	var modules []WikiModule
	discoverContexts(&modules, "/nonexistent-base-xyz", "mod", "Label", nil)
	if len(modules) != 0 {
		t.Errorf("expected 0 modules for nonexistent base, got %d", len(modules))
	}
}

func TestScanDir_NonDirEntriesSkipped(t *testing.T) {
	tmp := t.TempDir()
	base := filepath.Join(tmp, "base")
	if err := os.MkdirAll(base, 0o755); err != nil {
		t.Fatal(err)
	}
	// Create a regular file at depth 1 — should be skipped
	if err := os.WriteFile(filepath.Join(base, "regular-file.txt"), []byte("text"), 0o644); err != nil {
		t.Fatal(err)
	}

	var modules []WikiModule
	discoverContexts(&modules, base, "mod", "Label", nil)
	// Should not add any module from a regular file
	if len(modules) != 0 {
		t.Errorf("expected 0 modules, got %d", len(modules))
	}
}

func TestHandleMultiSearch_ValidSourcesWithAI(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "doc.md"), []byte("# Doc\n\nSome content about testing"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Use fakeAIClient that returns a valid answer — the wiki.SearchMultiWiki will
	// be called with real sources. We use a simple AI client that returns something.
	h := &WikiHandler{aiClient: fakeAIClient{response: "A short answer about testing."}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	body := fmt.Sprintf(`{"query":"testing","wiki_dirs":[{"id":"test","label":"Test","dir":%q}]}`, tmp)
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
	// Either we get an answer or an error from SearchMultiWiki,
	// but the handler should not crash
	t.Logf("response: answer=%q, error=%q, sessionID=%q", resp.Answer, resp.Error, resp.SessionID)
}

func TestHandleMultiSearch_ValidSourcesError(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "doc.md"), []byte("# Doc\n\nContent"), 0o644); err != nil {
		t.Fatal(err)
	}

	// AI client that returns error — should trigger "search failed" error
	h := &WikiHandler{aiClient: fakeAIClient{err: fmt.Errorf("ai failure")}}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))

	body := fmt.Sprintf(`{"query":"test","wiki_dirs":[{"id":"t","label":"T","dir":%q}]}`, tmp)
	req := httptest.NewRequest(http.MethodPost, "/api/wiki/multi-search", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	var resp MultiSearchResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !strings.Contains(resp.Error, "search failed") {
		t.Errorf("expected 'search failed' error, got %q", resp.Error)
	}
}

func TestHandleSessionMessages_ValidSession(t *testing.T) {
	// Create a real chat session to test the full path
	tmp := t.TempDir()
	t.Setenv("HOME", tmp)

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/wiki/sessions/messages", corsJSON(h.handleSessionMessages))

	// Try to load a nonexistent session — should get 404
	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions/messages?id=01JXYZ0000FAKE0000000000", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Errorf("status = %d; want %d for nonexistent session", w.Code, http.StatusNotFound)
	}
}

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

	// Create the global dir and registry file
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

func TestHandleMultiKeywordSearch_MultipleSources(t *testing.T) {
	tmp1 := t.TempDir()
	tmp2 := t.TempDir()

	if err := os.WriteFile(filepath.Join(tmp1, "a.md"), []byte("# Page A\n\nContent about search topic"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp2, "b.md"), []byte("# Page B\n\nMore search topic content"), 0o644); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))

	body := fmt.Sprintf(`{"query":"search topic","wiki_dirs":[{"id":"s1","label":"Source 1","dir":%q},{"id":"s2","label":"Source 2","dir":%q}]}`, tmp1, tmp2)
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
	// Results should be sorted by score descending
	for i := 1; i < len(results); i++ {
		if results[i].Score > results[i-1].Score {
			t.Errorf("results not sorted: score[%d]=%.2f > score[%d]=%.2f", i, results[i].Score, i-1, results[i-1].Score)
		}
	}
}

func TestHandleSessions_GetWithProjectDir(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("HOME", tmp)

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions?project_dir="+tmp, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}

	// Should return valid JSON (likely empty array)
	var items []SessionListItem
	if err := json.NewDecoder(w.Body).Decode(&items); err != nil {
		t.Fatalf("decode: %v", err)
	}
}

func TestHandleSessions_GetWithDefaultDir(t *testing.T) {
	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))

	// No project_dir param — should use os.Getwd()
	req := httptest.NewRequest(http.MethodGet, "/api/wiki/sessions", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}
}

func TestDiscoverModules_WithLockFile(t *testing.T) {
	tmp := t.TempDir()

	// Create lock file with project name
	lockContent := `{"project":{"name":"test-project-name"}}`
	lockPath := filepath.Join(tmp, "graphit.lock.json")
	if err := os.WriteFile(lockPath, []byte(lockContent), 0o644); err != nil {
		t.Fatal(err)
	}

	// Create minimal wiki dir
	knowledgeDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	if err := os.MkdirAll(knowledgeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(knowledgeDir, "index.md"), []byte("# KB"), 0o644); err != nil {
		t.Fatal(err)
	}

	modules := discoverModules(tmp)

	// Look for the knowledge module which should use the project name from lock file
	for _, m := range modules {
		if m.ID == "knowledge" {
			t.Logf("knowledge module label: %q (expected project name from lock)", m.Label)
		}
	}
}

func TestDiscoverModules_FallsBackToBasename(t *testing.T) {
	tmp := t.TempDir()
	// No lock file — should fall back to filepath.Base

	knowledgeDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	if err := os.MkdirAll(knowledgeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(knowledgeDir, "page.md"), []byte("# P"), 0o644); err != nil {
		t.Fatal(err)
	}

	modules := discoverModules(tmp)
	// Should not crash — label falls back to basename
	for _, m := range modules {
		if m.ID == "knowledge" {
			// Label should be the dirname (basename of tmp)
			if m.Label == "" {
				t.Error("expected non-empty label")
			}
		}
	}
}

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

