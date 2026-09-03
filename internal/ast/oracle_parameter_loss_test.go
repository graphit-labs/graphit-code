package ast

import (
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestOracleParametersReachTheCache guards against re-losing a third of the corpus.
//
// ConvertToCache drops parameters and fields whose Context is empty, so that an orphan
// parameter never reaches the graph. Context came from the match's IMMEDIATE PARENT, and
// for the pattern //parameter/parameter_name that parent is `parameter` — the owning
// function body sits several levels above. Every PL/SQL parameter therefore arrived
// without an owner and was discarded: 967 of 2732 entities across this sample, 35.4%,
// leaving functions with no HAS_PARAMETER edges and parameter names unsearchable.
//
// The matcher now carries the enclosing context down as it descends
// (Pattern.MatchWithContext), so the owner is found at any depth.
func TestOracleParametersReachTheCache(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory")
	}

	var files []string
	perDir := map[string]int{}
	_ = filepath.WalkDir(src, func(q string, d fs.DirEntry, e error) error {
		if e != nil || d.IsDir() || !strings.HasSuffix(strings.ToLower(q), ".sql") {
			return nil
		}
		if perDir[filepath.Dir(q)] >= 30 {
			return nil
		}
		perDir[filepath.Dir(q)]++
		files = append(files, q)
		return nil
	})
	if len(files) == 0 {
		t.Skip("no .sql files under GRAPHIT_E2E_SQL_DIR")
	}

	parsed, cached, params := 0, 0, 0
	for _, q := range files {
		c := NewCompositeParser(filepath.Dir(q), nil)
		f, err := c.Parse(q, false, ParseOptions{IndexSource: true})
		if err != nil || f == nil {
			continue
		}
		en := ConvertToCache(f, filepath.Dir(q), true, "")
		if en == nil {
			continue
		}
		parsed += f.EntityCount()
		cached += len(en.Entities)
		for _, e := range en.Entities {
			if e.Label == "Parameter" {
				params++
			}
		}
	}

	lost := parsed - cached
	t.Logf("%d files: parsed=%d cached=%d lost=%d (%.1f%%), parameters cached=%d",
		len(files), parsed, cached, lost, 100*float64(lost)/float64(parsed), params)

	if parsed == 0 {
		t.Fatal("nothing parsed — the test would pass vacuously")
	}
	if params == 0 {
		t.Error("no parameter reached the cache; they are being dropped again")
	}
	if lost*10 > parsed {
		t.Errorf("%d of %d entities (%.1f%%) did not reach the cache — context resolution has "+
			"regressed and owner-less parameters are being discarded again",
			lost, parsed, 100*float64(lost)/float64(parsed))
	}
}
