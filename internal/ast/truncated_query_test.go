package ast

import (
	"strings"
	"testing"
)

// Coverage for truncated queries.
//
// The index reaches them two ways: FTS5's prefix index (prefix='2 3 4' plus a `token*`
// pass) and the trigram bag. They cover different things — the prefix pass needs the query
// to be a prefix of a real token, the bag needs three characters — and this measures the
// union, so a regression in either shows up here.

// prefixCorpus merges the two corpora already in use so the probes cover both
// spelled-out and abbreviated naming, and adds Oracle-style names where truncation
// is the natural way to search.
func prefixCorpus() []gateEntity {
	out := append(gateCorpus(), abbrevCorpus()...)
	return append(out,
		gateEntity{"p1", "XPTO_EXTRAIR_ABCD01_DOC_LOTE", "Extrai lote de DOC.", "Procedure", "xpto.sql"},
		gateEntity{"p2", "PKG_VALIDACAO_PAGAMENTO", "Valida pagamento.", "Package", "pkg_val.sql"},
		gateEntity{"p3", "TRG_AUDITORIA_CLIENTE", "Auditoria de cliente.", "Trigger", "trg_aud.sql"},
	)
}

// TestTruncatedQueryCoverage checks that a truncated word still reaches the identifier it
// abbreviates, including the sub-trigram cases only the CONTAINS fallback can serve.
func TestTruncatedQueryCoverage(t *testing.T) {
	corpus := prefixCorpus()
	si := buildSearchIndexFrom(t, t.TempDir(), corpus)

	cases := []struct {
		query   string
		wantTop string
	}{
		// "valid" is deliberately absent: it is a prefix of both "validate" and
		// "validacao", so whichever of validateSchema and PKG_VALIDACAO_PAGAMENTO wins is
		// tie-breaking, not coverage. A probe with no defensible answer measures nothing.
		{"valida", "PKG_VALIDACAO_PAGAMENTO"},
		{"compu", "computeChecksum"},
		{"checks", "computeChecksum"},
		{"retr", "retryPolicy"},
		// "connect" rather than "data": "data" is a substring of Database in both
		// connectDatabase and closeDatabase, so no expected top-1 is defensible and the
		// probe would measure tie-breaking rather than truncation.
		{"connect", "connectDatabase"},
		{"audit", "TRG_AUDITORIA_CLIENTE"},
		{"extrair", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{"conf", "CONF_MGR"},
		// Shorter than a trigram, so the bag yields no gram: this one is served by the
		// prefix index alone ("cf" is a prefix of the token "cfg").
		{"cf", "CFG_LOAD"},
		// "db" is deliberately absent for the same reason as "valid": it matches
		// connectDatabase and closeDatabase equally.
	}

	t.Logf("%-10s | %-30s | %s", "query", "expected top-1", "got")
	t.Logf("%s", strings.Repeat("-", 76))

	var hits, empty int
	for _, cs := range cases {
		res, err := si.Search(cs.query, 5)
		if err != nil {
			t.Errorf("search %q: %v", cs.query, err)
			continue
		}
		names := entityNames(res, 5)
		top := ""
		if len(names) > 0 {
			top = names[0]
		} else {
			empty++
		}
		if top == cs.wantTop {
			hits++
		}
		shown := top
		if shown == "" {
			shown = "(vazio)"
		} else if top == cs.wantTop {
			shown += " OK"
		}
		t.Logf("%-10s | %-30s | %s", cs.query, cs.wantTop, shown)
	}

	t.Logf("%s", strings.Repeat("-", 76))
	t.Logf("expected top-1: %d/%d, empty: %d", hits, len(cases), empty)

	// Every remaining probe has one defensible answer, so the floor is all of them.
	const floor = 9
	if hits < floor {
		t.Errorf("truncated queries reached the expected entity %d/%d times, below the measured %d — "+
			"the trigram bag or the prefix pass has regressed", hits, len(cases), floor)
	}
	if empty > 0 {
		t.Errorf("%d truncated queries returned nothing; none did when this was measured", empty)
	}
}
