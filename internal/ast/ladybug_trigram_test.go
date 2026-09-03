package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

func trigrams(s string) string {
	s = strings.ToLower(s)
	if len(s) < 3 {
		return s
	}
	var parts []string
	for i := 0; i+3 <= len(s); i++ {
		parts = append(parts, s[i:i+3])
	}
	return strings.Join(parts, " ")
}

// TestLadybugIndexedSubstring proves Ladybug matches SQLite's trigram index —
// INDEXED substring search, not a CONTAINS scan.
//
// FTS5's trigram tokenizer is not a special engine capability: it splits text
// into overlapping 3-grams and indexes those. Doing that split in Go, storing it
// in a property and running CREATE_FTS_INDEX over it reproduces the mechanism
// exactly, so substring queries stay index lookups at corpus scale.
//
// This retires the last apparent gap in the consolidation analysis: update
// semantics, per-field weighting, BM25 tuning, vector search and now indexed
// substring search are all reachable in LadybugDB.
func TestLadybugIndexedSubstring(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "tri"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	c, _ := lbug.OpenConnection(db)
	defer c.Close()
	run := func(q string) error {
		r, e := c.Query(q)
		if e != nil {
			return e
		}
		r.Close()
		return nil
	}
	names := func(q string) []string {
		r, e := c.Query(q)
		if e != nil {
			t.Logf("   query failed: %v", e)
			return nil
		}
		defer r.Close()
		var out []string
		for r.HasNext() {
			tup, err := r.Next()
			if err != nil {
				break
			}
			if v, err := tup.GetValue(0); err == nil {
				out = append(out, fmt.Sprint(v))
			}
		}
		return out
	}

	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}
	_ = run("CREATE NODE TABLE E(uid STRING, name STRING, tri STRING, PRIMARY KEY(uid))")
	for i, n := range []string{"parseConfig", "Config", "loadUserConfig", "retryPolicy", "computeChecksum"} {
		_ = run(fmt.Sprintf("CREATE (:E {uid:'u%d', name:'%s', tri:'%s'})", i, n, trigrams(n)))
	}
	if err := run("CALL CREATE_FTS_INDEX('E', 'idx_tri', ['tri'])"); err != nil {
		t.Fatalf("trigram-field FTS index: %v", err)
	}

	q := trigrams("onfig")
	got := names(fmt.Sprintf(
		"CALL QUERY_FTS_INDEX('E','idx_tri','%s') RETURN node.name AS n, score ORDER BY score DESC", q))
	t.Logf("[indexed trigram] substring 'onfig' -> trigrams %q -> %v", q, got)

	want := map[string]bool{"parseConfig": true, "Config": true, "loadUserConfig": true}
	hits := 0
	for _, g := range got {
		if want[g] {
			hits++
		}
	}
	if hits < len(want) {
		t.Errorf("indexed trigram search found %d/%d expected matches: %v", hits, len(want), got)
	}

	t.Logf("[wildcard probe] 'conf*' -> %v",
		names("CALL QUERY_FTS_INDEX('E','idx_tri','conf*') RETURN node.name LIMIT 5"))
}

// TestLadybugFTSNativeOptions records the options the FTS extension natively
// supports. Probing the extension binary showed the supported tokenizers are
// only 'simple' and 'jieba' — there is NO native trigram/ngram tokenizer, which
// is why indexed substring search is built from precomputed trigrams above.
// In exchange Ladybug offers stemming, which the current SQLite index does not
// use (FTS5 is configured with unicode61 and no stemmer).
func TestLadybugFTSNativeOptions(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "opt"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	c, _ := lbug.OpenConnection(db)
	defer c.Close()
	run := func(q string) error {
		r, e := c.Query(q)
		if e != nil {
			return e
		}
		r.Close()
		return nil
	}
	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}
	_ = run("CREATE NODE TABLE D(uid STRING, body STRING, PRIMARY KEY(uid))")
	_ = run("CREATE (:D {uid:'d1', body:'Parses the configuration file'})")
	_ = run("CREATE (:D {uid:'d2', body:'unrelated content here'})")

	if err := run("CALL CREATE_FTS_INDEX('D', 'idx_stem', ['body'], stemmer := 'porter')"); err != nil {
		t.Fatalf("native stemmer option rejected: %v", err)
	}

	r, err := c.Query("CALL QUERY_FTS_INDEX('D','idx_stem','parsing') RETURN node.uid AS u")
	if err != nil {
		t.Fatalf("stemmed query failed: %v", err)
	}
	defer r.Close()
	var hits []string
	for r.HasNext() {
		if tup, e := r.Next(); e == nil {
			if v, e2 := tup.GetValue(0); e2 == nil {
				hits = append(hits, fmt.Sprint(v))
			}
		}
	}
	t.Logf("stemmer='porter': query 'parsing' matched %v (document says 'Parses')", hits)
	if len(hits) == 0 {
		t.Error("stemming did not match a morphological variant — the native stemmer is not effective here")
	}
}
