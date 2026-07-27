package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestOracleCommentsAreEntitiesAndSearchable covers the data dictionary, which the indexer
// used to throw away entirely.
//
// COMMENT ON statements were extracted only as a REFERENCES relation to the commented
// column or table, and the comment TEXT was never captured at all. In the reference Oracle
// export that is 2209 files — one per commented object, containing nothing else — which
// produced zero entities and zero searchable content.
//
// They are now Comment entities whose NAME is the comment text, so the documentation itself
// is what a developer searches for, and the commented object still gets its reference edge.
//
// Note for anyone changing plsql.yaml: query files are NOT embedded in the binary. They are
// read from ~/.graphit/runtime/<version>/ast/queries, so an edit in the repository has no
// effect until it is copied there — which is why this test reads the corpus rather than a
// fixture, and why it fails loudly instead of skipping when the queries are stale.
func TestOracleCommentsAreEntitiesAndSearchable(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}
	sample := filepath.Join(src, "comments", "EXEMPLO_OBJETO_UM.sql")
	if _, err := os.Stat(sample); err != nil {
		t.Skipf("no comments sample: %v", err)
	}

	comp := NewCompositeParser(filepath.Dir(sample), nil)
	pf, err := comp.Parse(sample, false, ParseOptions{IndexSource: true})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	entry := ConvertToCache(pf, filepath.Dir(sample), true, "")
	if entry == nil {
		t.Fatal("nothing cached")
	}

	var comments []cachedEntity
	for _, e := range entry.Entities {
		if e.Label == LabelComment {
			comments = append(comments, e)
		}
	}
	if len(comments) == 0 {
		t.Fatalf("no Comment entity extracted from %s — the COMMENT ON queries are not producing "+
			"entities (are the runtime query files up to date?)", filepath.Base(sample))
	}
	for _, c := range comments {
		t.Logf("Comment: %q", c.Name)
		if strings.HasPrefix(c.Name, "'") || strings.HasSuffix(c.Name, "'") {
			t.Errorf("comment name still carries its string delimiters: %q", c.Name)
		}
		if len(strings.Fields(c.Name)) < 2 {
			t.Errorf("comment name %q does not look like text — the pattern may be capturing the "+
				"commented object instead of the comment", c.Name)
		}
	}

	// The requirement: the CONTENT is searchable.
	dir := t.TempDir()
	cache, err := NewShardCache(filepath.Join(dir, "cache"))
	if err != nil {
		t.Fatalf("cache: %v", err)
	}
	if err := cache.Store(entry.RelPath, "h1", entry); err != nil {
		t.Fatalf("store: %v", err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	si := buildSearchIndex(t, dir, cache, nil)

	// A word from inside the comment, not from any identifier in the file.
	for _, query := range []string{"almoxarifado", "indicador"} {
		res, err := si.Search(query, 10)
		if err != nil {
			t.Fatalf("search %q: %v", query, err)
		}
		found := false
		for _, r := range res {
			if strings.Contains(strings.ToLower(r.Name), strings.ToLower(query)) {
				found = true
			}
		}
		t.Logf("search %-14s -> %d results, hit=%v", query, len(res), found)
		if !found {
			t.Errorf("the word %q from the comment text is not searchable", query)
		}
	}
}
