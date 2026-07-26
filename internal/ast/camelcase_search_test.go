package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// splitIdentifier turns "parseConfig" into "parseConfig parse Config" — the same
// trick the SQLite index already applies at write time, so both the original
// spelling and its parts are searchable as whole tokens.
func splitIdentifier(s string) string {
	var words []string
	cur := strings.Builder{}
	for _, r := range s {
		switch {
		case r == '_' || r == '-' || r == '.':
			if cur.Len() > 0 {
				words = append(words, cur.String())
				cur.Reset()
			}
		case r >= 'A' && r <= 'Z':
			if cur.Len() > 0 {
				words = append(words, cur.String())
				cur.Reset()
			}
			cur.WriteRune(r)
		default:
			cur.WriteRune(r)
		}
	}
	if cur.Len() > 0 {
		words = append(words, cur.String())
	}
	return s + " " + strings.Join(words, " ")
}

// TestCamelCaseSearchStrategies answers what a stemmer does NOT solve.
//
// Stemming normalises inflection of whole words (parsing -> pars). It does not
// break a compound identifier apart, so with raw camelCase indexed the query
// "config" scores 0/6 against names like parseConfig. Two things fix it, and
// they tie at 6/6:
//
//   - splitting the identifier at index time ("parseConfig parse Config"), which
//     the existing SQLite index already does; and
//   - a trigram index.
//
// The split wins on cost: roughly 2x a short name versus ~3x per character, no
// promiscuous matching as the corpus grows, and it is already implemented. So a
// corpus with camelCase still does not need trigrams — but it DOES need the
// split, which is therefore mandatory in any consolidation onto Ladybug FTS,
// not an optimisation.
func TestCamelCaseSearchStrategies(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "cc"), lbug.DefaultSystemConfig())
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
				if v, e2 := tup.GetValue(0); e2 == nil {
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
	_ = run("CREATE NODE TABLE C(uid STRING, name STRING, split STRING, tri STRING, PRIMARY KEY(uid))")

	ids := []string{"parseConfig", "loadUserProfile", "HttpRequestHandler", "computeChecksum", "validateSchemaVersion"}
	for i, n := range ids {
		_ = run(fmt.Sprintf("CREATE (:C {uid:'u%d', name:'%s', split:'%s', tri:'%s'})",
			i, n, splitIdentifier(n), trigrams(n)))
	}
	_ = run("CALL CREATE_FTS_INDEX('C','raw',['name'])")    // camelCase cru
	_ = run("CALL CREATE_FTS_INDEX('C','split',['split'])") // identificador dividido
	_ = run("CALL CREATE_FTS_INDEX('C','tri',['tri'])")     // trigrama

	cases := []struct{ query, want string }{
		{"config", "parseConfig"},
		{"profile", "loadUserProfile"},
		{"user", "loadUserProfile"},
		{"request", "HttpRequestHandler"},
		{"checksum", "computeChecksum"},
		{"version", "validateSchemaVersion"},
	}
	t.Logf("%-12s | %-7s %-7s %-7s", "query", "raw", "split", "trigram")
	t.Logf("%s", strings.Repeat("-", 42))
	var r, sp, tr int
	for _, cs := range cases {
		hit := func(idx, q string) string {
			res := names(fmt.Sprintf("CALL QUERY_FTS_INDEX('C','%s','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 3", idx, q))
			if len(res) > 0 && res[0] == cs.want {
				return "HIT"
			}
			if len(res) > 0 {
				return "wrong"
			}
			return "-"
		}
		a, b, d := hit("raw", cs.query), hit("split", cs.query), hit("tri", trigrams(cs.query))
		if a == "HIT" {
			r++
		}
		if b == "HIT" {
			sp++
		}
		if d == "HIT" {
			tr++
		}
		t.Logf("%-12s | %-7s %-7s %-7s", cs.query, a, b, d)
	}
	t.Logf("%s", strings.Repeat("-", 42))
	t.Logf("top-1: raw=%d/%d  split=%d/%d  trigram=%d/%d", r, len(cases), sp, len(cases), tr, len(cases))
}
