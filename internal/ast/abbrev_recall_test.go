package ast

import (
	"path/filepath"
	"strings"
	"testing"
)

// abbrevCorpus is the corpus TestAbbreviatedIdentifierSearch probes directly against a
// hand-built Ladybug FTS index, so the raw-versus-split-versus-trigram comparison and the
// whole-index measurements here run on identical input.
func abbrevCorpus() []gateEntity {
	return []gateEntity{
		{"a1", "coreConf", "Core configuration accessor.", "Function", "core.go"},
		{"a2", "CONF_MGR", "Configuration manager package.", "Package", "conf_mgr.sql"},
		{"a3", "CFG_LOAD", "Loads settings at startup.", "Procedure", "cfg_load.sql"},
		{"a4", "configLoader", "Loads the configuration.", "Class", "loader.go"},
		{"a5", "initConfiguration", "Initialises configuration state.", "Function", "init.go"},
		{"a6", "computeChecksum", "Computes a checksum.", "Function", "hash.go"},
		{"a7", "PKG_ACCOUNT_UPDATE", "Updates account rows.", "Package", "acct.sql"},
	}
}

// buildSearchIndexFrom seeds the search index with an arbitrary corpus.
func buildSearchIndexFrom(t *testing.T, dir string, corpus []gateEntity) *SearchIndex {
	t.Helper()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)
	return buildSearchIndex(t, dir, cache, nil)
}

// abbrevCorpusNamesOnly is abbrevCorpus with the prose stripped. It exists to remove a
// confound: every entity in abbrevCorpus has a docstring containing "configuration", and on
// the FTS5 index the prefix pass made the query "config" match that word, so a hit proved
// nothing about whether the NAME was reachable. That particular route is gone — LadybugDB
// has no prefix matching — but the variant is kept because prose can still answer a query
// through the docstring index, and because TestAbbreviatedIdentifierSearch indexes names
// only, so only this variant is comparable to it.
func abbrevCorpusNamesOnly() []gateEntity {
	out := abbrevCorpus()
	for i := range out {
		out[i].docstring = ""
	}
	return out
}

// abbrevProbes are the three directions of partial match, shared by every measurement of
// abbreviation recall.
func abbrevProbes() []struct {
	query string
	want  []string
} {
	return []struct {
		query string
		want  []string
	}{
		// Query longer than the stored token ("config" vs the token "conf").
		{"config", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		// Query shorter than the stored token ("conf" vs the token "config").
		{"conf", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		// Abbreviation sharing no substring with the query.
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

	// Guard the confound itself: if prose stops inflating the score, this test's
	// two-variant structure is no longer needed and the comment above is stale.
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

	// Both directions of partial overlap, by name only.
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
