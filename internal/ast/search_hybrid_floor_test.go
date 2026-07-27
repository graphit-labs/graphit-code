package ast

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// TestHybridSearchQualityFloor answers whether the semantic pass closes the gap the lexical
// one leaves, running the SAME probes as TestSearchIndexQualityFloor through both paths.
//
// Five of the sixteen probes are TIES, marked below: the corpus holds two entities with an
// equally good claim, so whichever comes first is tie-breaking, not quality. "config" matches
// Config exactly while the probe expects configLoader; "conf" is an exact token of both
// CONF_MGR and coreConf. Scoring those is meaningless, so parity is required only on the
// eleven probes that have one defensible answer — and the ties are still reported, because a
// change that flips several of them at once is worth seeing.
//
// This is also why 16/16 is not a target: reaching it would mean tuning the engine to prefer
// one arbitrary side of five coin flips.
func TestHybridSearchQualityFloor(t *testing.T) {
	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		if strings.Contains(err.Error(), "API version") {
			t.Fatalf("ONNX Runtime rejects the binding's API version — Makefile ORT_VERSION is out "+
				"of step with go.mod onnxruntime_go: %v", err)
		}
		t.Skipf("embedding client unavailable: %v", err)
	}
	qe, ok := client.(ai.QueryEmbedder)
	if !ok {
		t.Skip("client does not implement QueryEmbedder")
	}

	ctx := context.Background()
	dir := t.TempDir()
	corpus := prefixCorpus()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)

	// Mirror what the production embedder feeds the model: the entity's label, name and
	// docstring. Embedding less than production would understate the semantic pass.
	texts := make([]string, 0, len(corpus))
	uids := make([]string, 0, len(corpus))
	for _, e := range corpus {
		texts = append(texts, "["+e.entityType+"] "+e.path+"\n"+e.name+"\n"+e.docstring)
		uids = append(uids, e.uid)
	}
	vecs, err := client.EmbedBatch(ctx, texts)
	if err != nil {
		t.Skipf("embedding unavailable: %v", err)
	}
	byUID := make(map[string][]float32, len(uids))
	for i, uid := range uids {
		byUID[uid] = vecs[i]
	}

	si := buildSearchIndex(t, dir, cache, func(_, uid string) []float32 { return byUID[uid] })

	cases := []struct {
		query, wantTop string
		tie            string // non-empty: the equally defensible alternative
	}{
		{"parseConfig", "parseConfig", ""},
		{"configuration", "parseConfig", "initConfiguration"},
		{"schema", "validateSchema", "SchemaValidator"},
		{"checksum", "computeChecksum", ""},
		{"retry backoff", "retryPolicy", ""},
		{"parse sql", "parseSQL", ""},
		{"config", "configLoader", "Config"},
		{"conf", "CONF_MGR", "coreConf"},
		{"valid", "validateSchema", "SchemaValidator"},
		{"valida", "PKG_VALIDACAO_PAGAMENTO", ""},
		{"compu", "computeChecksum", ""},
		{"retr", "retryPolicy", ""},
		{"connect", "connectDatabase", ""},
		{"audit", "TRG_AUDITORIA_CLIENTE", ""},
		{"extrair", "XPTO_EXTRAIR_ABCD01_DOC_LOTE", ""},
		{"cf", "CFG_LOAD", ""},
	}

	t.Logf("%-16s | %-28s | %-24s | %-24s | %s", "query", "expected", "lexical", "hybrid", "tie with")
	t.Logf("%s", strings.Repeat("-", 116))

	var decisive, lexDecisive, hybDecisive, hybLostDecisive int
	lexHits, hybHits := 0, 0
	for _, c := range cases {
		lexRes, err := si.Search(c.query, 5)
		if err != nil {
			t.Fatalf("lexical %q: %v", c.query, err)
		}
		qv, err := qe.EmbedQuery(ctx, c.query)
		if err != nil {
			t.Fatalf("embed %q: %v", c.query, err)
		}
		hybRes, err := si.HybridSearch(c.query, qv, 5)
		if err != nil {
			t.Fatalf("hybrid %q: %v", c.query, err)
		}

		top := func(res []SearchResult) string {
			n := entityNames(res, 1)
			if len(n) == 0 {
				return ""
			}
			return n[0]
		}
		lex, hyb := top(lexRes), top(hybRes)
		lexOK, hybOK := lex == c.wantTop, hyb == c.wantTop
		if lexOK {
			lexHits++
		}
		if hybOK {
			hybHits++
		}

		// A tie is answered acceptably by either side; only the rest is decisive.
		accepts := func(got string) bool { return got == c.wantTop || (c.tie != "" && got == c.tie) }
		if c.tie == "" {
			decisive++
			if lexOK {
				lexDecisive++
			}
			if hybOK {
				hybDecisive++
			} else if lexOK {
				hybLostDecisive++
			}
		} else if !accepts(hyb) {
			t.Errorf("query %q returned %q, which is neither the expected %q nor the tie %q",
				c.query, hyb, c.wantTop, c.tie)
		}

		mark := func(s string, ok bool) string {
			if s == "" {
				return "(vazio)"
			}
			if ok {
				return s + " OK"
			}
			return s
		}
		t.Logf("%-16s | %-28s | %-24s | %-24s | %s",
			c.query, c.wantTop, mark(lex, lexOK), mark(hyb, hybOK), c.tie)
	}

	t.Logf("%s", strings.Repeat("-", 116))
	t.Logf("all %d probes: lexical %d, hybrid %d", len(cases), lexHits, hybHits)
	t.Logf("of the %d decisive probes: lexical %d, hybrid %d", decisive, lexDecisive, hybDecisive)

	// Fusion must add reach, not trade it. Losing a DECISIVE probe means the semantic pass
	// displaced an exact match — the failure the similarity floor was added to stop, when
	// both "cf" and "audit" came back as computeChecksum.
	if hybLostDecisive > 0 {
		t.Errorf("hybrid lost %d decisive probes the lexical pass answered — the semantic pass is "+
			"displacing exact matches", hybLostDecisive)
	}
	if hybDecisive < decisive {
		t.Errorf("hybrid answered %d of %d decisive probes", hybDecisive, decisive)
	}
}
