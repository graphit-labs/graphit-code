package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestStemmerVsTrigram measures where stemming and trigram indexing each win.
//
// They are NOT substitutes. Stemming normalises whole words (parsing/parsed ->
// pars), so it answers morphological variation; trigrams index overlapping
// 3-grams, so they answer partial words and typos — and incidentally cover much
// morphology too, since "parsing" and "parses" share trigrams.
//
// Two findings worth keeping:
//   - Ladybug enables stemming BY DEFAULT: an index built with no options
//     behaves like stemmer := 'porter', while stemmer := 'none' collapses from
//     4/7 to 1/7. Stemming is therefore not something consolidation gains, it is
//     already the baseline.
//   - Trigram scores 7/7 here, but on THREE documents. Trigrams match
//     promiscuously, so a tiny corpus flatters their precision. Treat this as
//     evidence of recall, not of overall quality; the real-corpus gate decides
//     the weighting between the two indexes.
func TestStemmerVsTrigram(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "cmp"), lbug.DefaultSystemConfig())
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
			return nil
		}
		defer r.Close()
		var out []string
		for r.HasNext() {
			if tup, err := r.Next(); err == nil {
				if v, err2 := tup.GetValue(0); err2 == nil {
					out = append(out, fmt.Sprint(v))
				}
			}
		}
		return out
	}

	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}
	_ = run("CREATE NODE TABLE E(uid STRING, name STRING, doc STRING, tri STRING, PRIMARY KEY(uid))")

	corpus := []struct{ uid, name, doc string }{
		{"u1", "parseConfig", "Parses the configuration file."},
		{"u2", "validateSchema", "Validates the database schema."},
		{"u3", "retryPolicy", "Retries failed network calls."},
	}
	for _, e := range corpus {
		blob := e.name + " " + e.doc
		_ = run(fmt.Sprintf("CREATE (:E {uid:'%s', name:'%s', doc:'%s', tri:'%s'})",
			e.uid, e.name, strings.ReplaceAll(e.doc, "'", "''"), trigrams(blob)))
	}
	_ = run("CALL CREATE_FTS_INDEX('E','plain',['name','doc'])")
	_ = run("CALL CREATE_FTS_INDEX('E','stem',['name','doc'], stemmer := 'porter')")
	_ = run("CALL CREATE_FTS_INDEX('E','tri',['tri'])")
	_ = run("CALL CREATE_FTS_INDEX('E','nostem',['name','doc'], stemmer := 'none')")

	probe := func(idx, q string) []string {
		return names(fmt.Sprintf("CALL QUERY_FTS_INDEX('E','%s','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 3", idx, q))
	}

	cases := []struct{ kind, query, want string }{
		{"morphological", "parsing", "parseConfig"}, // 'Parses' in doc
		{"morphological", "validation", "validateSchema"},
		{"morphological", "retrying", "retryPolicy"},
		{"substring", "onfig", "parseConfig"}, // partial word
		{"substring", "chema", "validateSchema"},
		{"typo", "confg", "parseConfig"}, // missing letter
		{"exact word", "configuration", "parseConfig"},
	}

	t.Logf("%-14s %-14s | %-9s %-9s %-9s", "kind", "query", "plain", "stemmer", "trigram")
	t.Logf("%s", strings.Repeat("-", 64))
	score := map[string]int{}
	for _, cs := range cases {
		hit := func(idx string, q string) string {
			r := probe(idx, q)
			if len(r) > 0 && r[0] == cs.want {
				score[idx]++
				return "HIT"
			}
			if len(r) > 0 {
				return "wrong"
			}
			return "-"
		}
		p, s, tr, ns := hit("plain", cs.query), hit("stem", cs.query), hit("tri", trigrams(cs.query)), hit("nostem", cs.query)
		t.Logf("%-14s %-14s | %-9s %-9s %-9s %-9s", cs.kind, cs.query, p, s, tr, ns)
	}
	t.Logf("%s", strings.Repeat("-", 64))
	t.Logf("top-1 hits: plain=%d  stemmer=%d  trigram=%d  NOstemmer=%d  (of %d)",
		score["plain"], score["stem"], score["tri"], score["nostem"], len(cases))
}
