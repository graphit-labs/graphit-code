package ast

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// NOTE: probe identifiers in this file are synthetic, and should stay that way.
// These tests seed their own database, so any identifier of the right shape
// serves the purpose — the measurement is whether a fragment of a compound name
// finds it. Keeping them synthetic also keeps the tests independent of whatever
// corpus GRAPHIT_E2E_SQL_DIR happens to point at.

// TestHybridSearchQualityFloor answers whether the semantic pass closes the gap the lexical
// one leaves, running the SAME probes as TestSearchIndexQualityFloor through both paths.
//
// Five of the sixteen probes are AMBIGUOUS, marked below: the corpus holds more than one entity
// with an equally good claim, so whichever comes first is tie-breaking, not quality. "config"
// matches Config exactly while the probe expects configLoader; "conf" is an exact token of both
// CONF_MGR and coreConf. Scoring those is meaningless, so top-1 is required only on the eleven
// probes that have one defensible answer. The ambiguous ones only have to return ONE OF the
// defensible answers — which rejects an unrelated entity at rank one without pretending to know
// which of several equals should win.
//
// This is also why 16/16 is not a target: reaching it would mean tuning the engine to prefer one
// arbitrary side of five coin flips.
//
// WHAT THIS GATE MEASURED THE FIRST TIME IT ACTUALLY RAN (2026-08-24). It had been skipping
// silently, because ORT was only reachable from inside the launcher payload — so a whole channel
// went unmeasured. Against the commit before the score-column fix it scored **0 of 11 decisive
// probes**: "cf" returned parseConfig, "connect" returned CFG_LOAD, "audit" returned
// computeChecksum. Not weak ranking — no ranking, because lancestore was reading the hybrid score
// out of whichever of two columns Go's map iteration reached last. With that fixed it is 11 of 11,
// one better than the lexical pass. A skipping gate cost more than a missing one would have: it
// reported success over a search channel that was returning noise.
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
		// ambiguous names the OTHER entities in this corpus with an equally defensible claim
		// on the query. Non-empty means the probe cannot assert WHICH answer wins, only that
		// the winner is one of them — the reason being the one written down in
		// truncated_query_test.go: a probe whose top-1 is a tie-break measures nothing.
		//
		// It was a single string, which could name only ONE alternative and so understated
		// how ambiguous some of these are — and that is the whole reason this gate failed the
		// first time it ran. "configuration" is carried by SEVEN entities in this corpus, one
		// of them (Config) with the docstring "Configuration for the parser." — at least as
		// defensible as the expected parseConfig, and not expressible in one string.
		ambiguous []string
	}{
		{"parseConfig", "parseConfig", nil},
		// Seven entities carry "configuration", in the name or the docstring.
		{"configuration", "parseConfig", []string{
			"Config", "configLoader", "initConfiguration", "loadUserConfig", "coreConf", "CONF_MGR",
		}},
		{"schema", "validateSchema", []string{"SchemaValidator"}},
		{"checksum", "computeChecksum", nil},
		{"retry backoff", "retryPolicy", nil},
		{"parse sql", "parseSQL", nil},
		{"config", "configLoader", []string{"Config", "parseConfig", "loadUserConfig"}},
		{"conf", "CONF_MGR", []string{"coreConf"}},
		{"valid", "validateSchema", []string{"SchemaValidator"}},
		{"valida", "PKG_VALIDACAO_PAGAMENTO", nil},
		{"compu", "computeChecksum", nil},
		{"retr", "retryPolicy", nil},
		{"connect", "connectDatabase", nil},
		{"audit", "TRG_AUDITORIA_CLIENTE", nil},
		{"extrair", "XPTO_EXTRAIR_ABCD01_DOC_LOTE", nil},
		{"cf", "CFG_LOAD", nil},
	}

	t.Logf("%-16s | %-28s | %-24s | %-24s | %s", "query", "expected", "lexical", "hybrid", "equally defensible")
	t.Logf("%s", strings.Repeat("-", 116))

	var decisive, lexDecisive, hybDecisive, hybLostDecisive int
	lexHits, hybHits := 0, 0
	for _, c := range cases {
		lexRes, err := si.Search(context.Background(), c.query, 5)
		if err != nil {
			t.Fatalf("lexical %q: %v", c.query, err)
		}
		qv, err := qe.EmbedQuery(ctx, c.query)
		if err != nil {
			t.Fatalf("embed %q: %v", c.query, err)
		}
		hybRes, err := si.HybridSearch(context.Background(), c.query, qv, 5)
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

		// Only an unambiguous probe can assert WHICH entity wins. An ambiguous one asserts
		// that whatever won is one of the defensible answers — which still rejects an
		// unrelated entity at rank one, and still refuses to pick a winner among equals.
		//
		// Requiring the expected entity inside a recall window was tried and is WRONG here:
		// "configuration" has seven defensible answers and the window is five, so the
		// requirement just moves the tie-break from position 1 to position 5. Measured, it
		// failed on a hybrid answer set of five configuration entities — a good answer that
		// happened not to include the one the row names first.
		defensible := append([]string{c.wantTop}, c.ambiguous...)
		if len(c.ambiguous) == 0 {
			decisive++
			if lexOK {
				lexDecisive++
			}
			if hybOK {
				hybDecisive++
			} else if lexOK {
				hybLostDecisive++
			}
		} else if !hasName(defensible, hyb) {
			t.Errorf("query %q returned %q, which is none of the defensible answers %v",
				c.query, hyb, defensible)
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
			c.query, c.wantTop, mark(lex, lexOK), mark(hyb, hybOK), strings.Join(c.ambiguous, " "))
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
