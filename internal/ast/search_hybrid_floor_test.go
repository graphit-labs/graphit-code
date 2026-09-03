//go:build lancedb

package ast

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

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
		ambiguous      []string
	}{
		{"parseConfig", "parseConfig", nil},
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

	if hybLostDecisive > 0 {
		t.Errorf("hybrid lost %d decisive probes the lexical pass answered — the semantic pass is "+
			"displacing exact matches", hybLostDecisive)
	}
	if hybDecisive < decisive {
		t.Errorf("hybrid answered %d of %d decisive probes", hybDecisive, decisive)
	}
}
