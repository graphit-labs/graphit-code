//go:build lancedb

package ast

import (
	"path/filepath"
	"strings"
	"testing"
)

func buildSearchIndexFrom(t *testing.T, dir string, corpus []gateEntity) *SearchIndex {
	t.Helper()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)
	return buildSearchIndex(t, dir, cache, nil)
}

func abbrevProbes() []struct {
	query string
	want  []string
} {
	return []struct {
		query string
		want  []string
	}{
		{"config", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		{"conf", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		{"config", []string{"CFG_LOAD"}},
	}
}

// TestAbbreviatedIdentifierRecall measures how far the shipping index reaches an
// abbreviated identifier from a spelled-out query, with and without prose.
//
// The two rows exist because the question was originally answered wrong: with docstrings
// present the FTS5 index scored 8/9, and the credit belonged to its prefix index matching
// "configuration" in the prose rather than to the identifier. Only the names-only row says
// what the NAME index can reach, and it is the row comparable to
// TestAbbreviatedIdentifierSearch.
func TestAbbreviatedIdentifierRecall(t *testing.T) {
	variants := []struct {
		label  string
		corpus []gateEntity
	}{
		{"with docstrings", abbrevCorpus()},
		{"names only", abbrevCorpusNamesOnly()},
	}

	totals := map[string][2]int{}

	for _, v := range variants {
		si := buildSearchIndexFrom(t, t.TempDir(), v.corpus)

		t.Logf("=== %s ===", v.label)
		t.Logf("%-8s | %-46s | found", "query", "expected")
		t.Logf("%s", strings.Repeat("-", 92))

		total, wantTotal := 0, 0
		for _, cs := range abbrevProbes() {
			got := indexSearchNames(t, si, cs.query, 10)
			wantSet := map[string]bool{}
			for _, w := range cs.want {
				wantSet[w] = true
			}
			n := 0
			for _, g := range got {
				if wantSet[g] {
					n++
				}
			}
			total += n
			wantTotal += len(cs.want)
			t.Logf("%-8s | %-46s | %d/%d  -> %v", cs.query, strings.Join(cs.want, ","), n, len(cs.want), got)
		}
		t.Logf("recall over %d expected matches: %d", wantTotal, total)
		totals[v.label] = [2]int{total, wantTotal}

		if wantTotal == 0 {
			t.Fatal("no expectations were probed — the test measured nothing")
		}
	}

	withDoc, namesOnly := totals["with docstrings"], totals["names only"]
	t.Logf("%s", strings.Repeat("-", 92))
	t.Logf("index recall: with docstrings %d/%d, names only %d/%d",
		withDoc[0], withDoc[1], namesOnly[0], namesOnly[1])

	if namesOnly[0] >= withDoc[0] {
		t.Logf("NOTE: docstrings no longer inflate abbreviation recall (names-only %d >= with-docstrings %d) — "+
			"the prose confound this test isolates has disappeared",
			namesOnly[0], withDoc[0])
	}
}

// TestAbbreviationRecallByNameAlone is the requirement, not a comparison: a
// developer typing the spelled-out word must reach the abbreviated identifier
// through its NAME, with no help from prose.
//
// A trigram TOKENIZER cannot do this, which is why the index does not use one. FTS5's
// matched the query's trigrams as an ordered phrase — substring containment — so "config"
// never reached coreConf, which lacks nfi/fig. Scoring an unordered BAG of trigrams with
// BM25 does reach it, because partial overlap still ranks: trigrams are precomputed at
// write time into name_tri and indexed with a word tokenizer, so "con onf nfi fig"
// OR-matches "cor ore rec eco con onf".
func TestAbbreviationRecallByNameAlone(t *testing.T) {
	si := buildSearchIndexFrom(t, t.TempDir(), abbrevCorpusNamesOnly())

	for _, cs := range []struct {
		query string
		want  []string
	}{
		{"config", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		{"conf", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
	} {
		got := indexSearchNames(t, si, cs.query, 10)
		gotSet := map[string]bool{}
		for _, g := range got {
			gotSet[g] = true
		}
		var missing []string
		for _, w := range cs.want {
			if !gotSet[w] {
				missing = append(missing, w)
			}
		}
		t.Logf("%-8s -> %v", cs.query, got)
		if len(missing) > 0 {
			t.Errorf("query %q did not reach %v by name alone (got %v)", cs.query, missing, got)
		}
	}
}
