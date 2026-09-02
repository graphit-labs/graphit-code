//go:build lancedb

package uiserver

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestHandleSearch_FallbackLinearSearch(t *testing.T) {
	t.Parallel()
	tmp := t.TempDir()

	content := "ab"
	if err := indexPage(t, tmp, "tiny.md", content); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/search?dir="+tmp+"&q=ab", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("status = %d; want %d", w.Code, http.StatusOK)
	}

	var results []SearchResult
	if err := json.NewDecoder(w.Body).Decode(&results); err != nil {
		t.Fatalf("decode: %v", err)
	}
	t.Logf("Got %d results for tiny file search", len(results))
}

// handleMultiSearch additional edge cases

// The module-discovery tests that used to live here built wikis inside the project —
// `.graphit/knowledge/...`, `.graphit/memory/...` — which is where they lived before the
// storage centralization and where nothing lives now. Their replacements are in
// wiki_modules_test.go, against an isolated global store.
