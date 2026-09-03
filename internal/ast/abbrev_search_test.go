package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestAbbreviatedIdentifierSearch probes the case the earlier corpus never
// exercised: the query and the identifier share a PREFIX but not a whole token.
//
// TestOracleIdentifierSearch and TestCamelCaseSearchStrategies both asked for a
// word that appears complete inside the name ("config" in parseConfig), which
// index-time splitting turns into an exact token match. That is why trigrams
// looked dispensable. Real code also abbreviates — coreConf, CONF_MGR, CFG_LOAD —
// and then:
//
//   - the stored token is "conf" while the query is "config";
//   - porter stemming does not truncate "config" to "conf";
//   - there is no wildcard operator (the 'conf*' probe in
//     TestLadybugIndexedSubstring returned nothing).
//
// So exact-token FTS cannot bridge it in either direction. Trigrams can, because
// "con"/"onf" are shared. This test measures both directions per strategy so the
// consolidation decision rests on numbers rather than on the earlier corpus.
func TestAbbreviatedIdentifierSearch(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "abbrev"), lbug.DefaultSystemConfig())
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
	if err := run("CREATE NODE TABLE A(uid STRING, name STRING, split STRING, tri STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema: %v", err)
	}

	ids := []string{
		"coreConf",
		"CONF_MGR",
		"CFG_LOAD",
		"configLoader",
		"initConfiguration",
		"computeChecksum",
		"PKG_ACCOUNT_UPDATE",
	}
	for i, n := range ids {
		if err := run(fmt.Sprintf("CREATE (:A {uid:'u%d', name:'%s', split:'%s', tri:'%s'})",
			i, n, splitIdentifier(n), trigrams(n))); err != nil {
			t.Fatalf("seed %s: %v", n, err)
		}
	}
	for _, ddl := range []string{
		"CALL CREATE_FTS_INDEX('A','raw',['name'])",
		"CALL CREATE_FTS_INDEX('A','split',['split'])",
		"CALL CREATE_FTS_INDEX('A','tri',['tri'])",
	} {
		if err := run(ddl); err != nil {
			t.Fatalf("index %q: %v", ddl, err)
		}
	}

	cases := []struct {
		query string
		want  []string
	}{
		{"config", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		{"conf", []string{"coreConf", "CONF_MGR", "configLoader", "initConfiguration"}},
		{"config", []string{"CFG_LOAD"}},
	}

	hitsFor := func(index, query string, want []string) (found int, got []string) {
		q := query
		if index == "tri" {
			q = trigrams(query)
		}
		got = names(fmt.Sprintf(
			"CALL QUERY_FTS_INDEX('A','%s','%s') RETURN node.name AS n, score ORDER BY score DESC LIMIT 7",
			index, escapeQuote(q)))
		wantSet := map[string]bool{}
		for _, w := range want {
			wantSet[w] = true
		}
		for _, g := range got {
			if wantSet[g] {
				found++
			}
		}
		return found, got
	}

	t.Logf("%-8s | %-28s | %-5s %-5s %-5s | recall of expected", "query", "expected", "raw", "split", "tri")
	t.Logf("%s", strings.Repeat("-", 100))

	var rawTotal, splitTotal, triTotal, wantTotal int
	for _, cs := range cases {
		rawN, rawGot := hitsFor("raw", cs.query, cs.want)
		splitN, splitGot := hitsFor("split", cs.query, cs.want)
		triN, triGot := hitsFor("tri", cs.query, cs.want)

		rawTotal += rawN
		splitTotal += splitN
		triTotal += triN
		wantTotal += len(cs.want)

		t.Logf("%-8s | %-28s | %-5s %-5s %-5s | raw=%d/%d split=%d/%d tri=%d/%d",
			cs.query, strings.Join(cs.want, ","),
			fmt.Sprintf("%d", rawN), fmt.Sprintf("%d", splitN), fmt.Sprintf("%d", triN),
			rawN, len(cs.want), splitN, len(cs.want), triN, len(cs.want))
		t.Logf("         raw   -> %v", rawGot)
		t.Logf("         split -> %v", splitGot)
		t.Logf("         tri   -> %v", triGot)
	}

	t.Logf("%s", strings.Repeat("-", 100))
	t.Logf("recall over %d expected matches: raw=%d split=%d trigram=%d",
		wantTotal, rawTotal, splitTotal, triTotal)

	if wantTotal == 0 {
		t.Fatal("no expectations were probed — the test measured nothing")
	}

	if triTotal <= splitTotal {
		t.Errorf("trigram (%d) did not beat split (%d) on abbreviated identifiers — "+
			"if this holds, the FTS+split-only design is sufficient after all",
			triTotal, splitTotal)
	}
}
