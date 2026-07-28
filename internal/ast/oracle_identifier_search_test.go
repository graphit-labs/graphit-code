package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// NOTE: probe identifiers in this file are synthetic, and should stay that way.
// These tests seed their own database, so any identifier of the right shape
// serves the purpose — the measurement is whether a fragment of a compound name
// finds it. Keeping them synthetic also keeps the tests independent of whatever
// corpus GRAPHIT_E2E_SQL_DIR happens to point at.

// TestOracleIdentifierSearch decides whether a trigram index is worth carrying
// for THIS corpus.
//
// Trigram indexing pays off when queries are partial words that word
// tokenisation cannot reach. The target corpus is Oracle PL/SQL whose
// identifiers are underscore-separated abbreviations
// (XPTO_EXTRAIR_ABCD01_DOC_LOTE), and the FTS tokenizer splits on underscores —
// so a developer searching "ABCD01", "DOC" or "ENTRG" is already searching whole
// tokens. Measured here: plain FTS 6/6, trigram 6/6, i.e. the trigram index adds
// nothing while costing an extra property, an extra index and precision at
// scale (trigrams match promiscuously on large corpora).
//
// Conclusion: for this corpus, consolidate onto Ladybug FTS alone — stemming is
// on by default and per-field weighting comes from one index per field. A
// camelCase-heavy corpus could reach a different answer; rerun this to check.
func TestOracleIdentifierSearch(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "ora"), lbug.DefaultSystemConfig())
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
	_ = run("CREATE NODE TABLE O(uid STRING, name STRING, tri STRING, PRIMARY KEY(uid))")

	real := []string{
		"XPTO_EXTRAIR_ABCD01_DOC_LOTE",
		"XPTO_EXT_ABCD02_DOC_DSTR_LOTE",
		"VERIF_ZZP_SORTEIA_BX",
		"VW_RESUMO_ITEM_XYZW_NN",
		"CALC_DT_PREV_ENTRG_DEPOS_EST",
		"VALIDA_ITEM_COND_ENTRG_PEDIDO",
	}
	for i, n := range real {
		_ = run(fmt.Sprintf("CREATE (:O {uid:'u%d', name:'%s', tri:'%s'})", i, n, trigrams(n)))
	}
	_ = run("CALL CREATE_FTS_INDEX('O','fts',['name'])")
	_ = run("CALL CREATE_FTS_INDEX('O','tri',['tri'])")

	// Queries a developer actually types: a fragment of the compound name.
	cases := []struct{ query, want string }{
		{"ABCD01", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{"SORTEIA", "VERIF_ZZP_SORTEIA_BX"},
		{"DOC", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{"RESUMO_ITEM", "VW_RESUMO_ITEM_XYZW_NN"},
		{"ENTRG", "VALIDA_ITEM_COND_ENTRG_PEDIDO"},
		{"XPTO_EXTRAIR_ABCD01_DOC_LOTE", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
	}
	t.Logf("%-30s | %-8s %-8s", "query", "FTS", "trigram")
	t.Logf("%s", strings.Repeat("-", 52))
	f, tr := 0, 0
	for _, cs := range cases {
		fr := names(fmt.Sprintf("CALL QUERY_FTS_INDEX('O','fts','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 3", cs.query))
		trr := names(fmt.Sprintf("CALL QUERY_FTS_INDEX('O','tri','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 3", trigrams(cs.query)))
		fh, th := "-", "-"
		if len(fr) > 0 && fr[0] == cs.want {
			fh = "HIT"
			f++
		} else if len(fr) > 0 {
			fh = "wrong"
		}
		if len(trr) > 0 && trr[0] == cs.want {
			th = "HIT"
			tr++
		} else if len(trr) > 0 {
			th = "wrong"
		}
		t.Logf("%-30s | %-8s %-8s", cs.query, fh, th)
	}
	t.Logf("%s", strings.Repeat("-", 52))
	t.Logf("top-1: FTS=%d/%d  trigram=%d/%d", f, len(cases), tr, len(cases))
}
