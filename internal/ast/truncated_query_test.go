//go:build lancedb

package ast

import (
	"context"
	"strings"
	"testing"
)

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
		{"compu", "computeChecksum"},
		{"checks", "computeChecksum"},
		{"retr", "retryPolicy"},
		{"connect", "connectDatabase"},
		{"audit", "TRG_AUDITORIA_CLIENTE"},
		{"extrair", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{"conf", "CONF_MGR"},
		{"cf", "CFG_LOAD"},
	}

	recallOnly := []struct {
		query   string
		wantAny string
	}{
		{"valida", "PKG_VALIDACAO_PAGAMENTO"},
	}

	t.Logf("%-10s | %-30s | %s", "query", "expected top-1", "got")
	t.Logf("%s", strings.Repeat("-", 76))

	var hits, empty int
	for _, cs := range cases {
		res, err := si.Search(context.Background(), cs.query, 5)
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

	for _, cs := range recallOnly {
		res, err := si.Search(context.Background(), cs.query, 5)
		if err != nil {
			t.Errorf("search %q: %v", cs.query, err)
			continue
		}
		names := entityNames(res, 5)
		found := false
		for _, n := range names {
			if n == cs.wantAny {
				found = true
			}
		}
		t.Logf("%-10s | %-30s | recall: %v (%v)", cs.query, cs.wantAny, found, names)
		if !found {
			t.Errorf("the truncated query %q does not reach %q at all, in any position: %v",
				cs.query, cs.wantAny, names)
		}
	}

	const floor = 8
	if hits < floor {
		t.Errorf("truncated queries reached the expected entity %d/%d times, below the measured %d — "+
			"the trigram bag or the prefix pass has regressed", hits, len(cases), floor)
	}
	if empty > 0 {
		t.Errorf("%d truncated queries returned nothing; none did when this was measured", empty)
	}
}
