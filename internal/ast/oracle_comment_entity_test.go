//go:build lancedb

package ast

import (
	"context"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

func TestOracleCommentsAreEntitiesAndSearchable(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}
	matches, err := filepath.Glob(filepath.Join(src, "comments", "*.sql"))
	if err != nil || len(matches) == 0 {
		t.Skipf("no comments sample under %s: %v", filepath.Join(src, "comments"), err)
	}
	sort.Strings(matches)
	sample := matches[0]

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

	for _, query := range []string{"almoxarifado", "indicador"} {
		res, err := si.Search(context.Background(), query, 10)
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
