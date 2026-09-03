package ast

import (
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestOracleCorpusExtraction(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}

	var samples []string
	seenDir := map[string]bool{}
	_ = filepath.WalkDir(src, func(p string, d fs.DirEntry, e error) error {
		if e != nil || d.IsDir() || !strings.HasSuffix(strings.ToLower(p), ".sql") {
			return nil
		}
		dir := filepath.Dir(p)
		if seenDir[dir] {
			return nil
		}
		seenDir[dir] = true
		samples = append(samples, p)
		if len(samples) >= 6 {
			return filepath.SkipAll
		}
		return nil
	})
	if len(samples) == 0 {
		t.Skip("no .sql files under GRAPHIT_E2E_SQL_DIR")
	}

	opts := ParseOptions{IndexSource: true}

	t.Logf("%-42s | %-22s | %s", "file", "engine", "entities")
	t.Logf("%s", strings.Repeat("-", 92))

	totals := map[string]int{}
	for _, p := range samples {
		rel, _ := filepath.Rel(src, p)
		if len(rel) > 40 {
			rel = "…" + rel[len(rel)-39:]
		}

		comp := NewCompositeParser(filepath.Dir(p), nil)
		pf, err := comp.Parse(p, false, opts)
		n, engine := 0, "(nil)"
		if err != nil {
			engine = "error: " + err.Error()
		} else if pf != nil {
			n, engine = pf.EntityCount(), pf.Parser
		}
		totals["composite"] += n
		t.Logf("%-42s | %-22s | %d", rel, engine, n)

		for _, grammar := range []string{"antlr-plsql", "tree-sitter-sql"} {
			pinned := NewCompositeParser(filepath.Dir(p), map[string]string{".sql": grammar})
			pf, err := pinned.Parse(p, false, opts)
			n := 0
			label := grammar
			if err != nil {
				label = grammar + " (error: " + firstLine(err.Error()) + ")"
			} else if pf != nil {
				n = pf.EntityCount()
			}
			totals[grammar] += n
			t.Logf("%-42s | %-22s | %d", "", label, n)
		}
	}

	t.Logf("%s", strings.Repeat("-", 92))
	t.Logf("entities over %d files: composite=%d antlr-plsql=%d tree-sitter-sql=%d",
		len(samples), totals["composite"], totals["antlr-plsql"], totals["tree-sitter-sql"])

	if totals["composite"] == 0 {
		t.Errorf("the composite parser extracted NO entities from %d real Oracle files — "+
			"every measurement taken through the search index describes an empty index",
			len(samples))
	}
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i > 0 {
		return s[:i]
	}
	if len(s) > 60 {
		return s[:60] + "…"
	}
	return s
}
