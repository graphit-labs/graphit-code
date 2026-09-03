//go:build lancedb

package ast

import (
	"context"
	"math"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

func abbrevExpansions(cfgLoadExpansion string) map[string]string {
	return map[string]string{
		"coreConf":           "core configuration",
		"CONF_MGR":           "configuration manager",
		"CFG_LOAD":           cfgLoadExpansion,
		"configLoader":       "config loader",
		"initConfiguration":  "initialise configuration",
		"computeChecksum":    "compute checksum",
		"PKG_ACCOUNT_UPDATE": "package account update",
	}
}

func TestExpansionFieldCeiling(t *testing.T) {
	variants := []struct {
		label, cfgLoad string
	}{
		{"morphological variant (\"configuration load\")", "configuration load"},
		{"exact query token (\"config load\")", "config load"},
	}

	results := map[string]int{}

	for _, v := range variants {
		corpus := abbrevCorpusNamesOnly()
		exp := abbrevExpansions(v.cfgLoad)
		for i := range corpus {
			corpus[i].docstring = exp[corpus[i].name]
		}
		si := buildSearchIndexFrom(t, t.TempDir(), corpus)

		t.Logf("=== %s ===", v.label)
		total, wantTotal := 0, 0
		for _, cs := range abbrevProbes() {
			got := indexSearchNames(t, si, cs.query, 10)
			wantSet := map[string]bool{}
			for _, w := range cs.want {
				wantSet[w] = true
			}
			n := 0
			for _, g := range got {
				if wantSet[g] {
					n++
				}
			}
			total += n
			wantTotal += len(cs.want)
			t.Logf("  %-8s | %-46s | %d/%d -> %v", cs.query, strings.Join(cs.want, ","), n, len(cs.want), got)
		}
		t.Logf("  recall: %d/%d (without the prefix index this wording reaches 8/9)", total, wantTotal)
		results[v.label] = total

		if wantTotal == 0 {
			t.Fatal("no expectations were probed — the test measured nothing")
		}
	}

	morph := results["morphological variant (\"configuration load\")"]
	exact := results["exact query token (\"config load\")"]
	t.Logf("expansion recall: morphological %d, exact-token %d", morph, exact)

	if exact != 9 {
		t.Errorf("the exact-token wording scored %d/9, expected 9/9. This wording repeats the "+
			"searcher's exact word, so it should reach every probe without any expansion at all",
			exact)
	}
	if exact < morph {
		t.Errorf("the morphological wording (%d) beat the exact-token one (%d), which should be "+
			"impossible: repeating the searcher's word cannot be worse than not repeating it. "+
			"Something has changed about how the query reaches the index", morph, exact)
	}
	if exact-morph > 1 {
		t.Errorf("an expansion field is now worth %d probes (exact %d vs morphological %d), not "+
			"the one it was worth when this was measured. Above one, the case for building it "+
			"has to be re-argued rather than assumed closed", exact-morph, exact, morph)
	}
}

func cosine(a, b []float32) float64 {
	var dot, na, nb float64
	for i := range a {
		dot += float64(a[i]) * float64(b[i])
		na += float64(a[i]) * float64(a[i])
		nb += float64(b[i]) * float64(b[i])
	}
	if na == 0 || nb == 0 {
		return 0
	}
	return dot / (math.Sqrt(na) * math.Sqrt(nb))
}

// TestSemanticReachOfAbbreviations asks whether the embedding path ALREADY does
// what an expansion field would be built for. It ranks the corpus names by cosine
// similarity to the query, using the same client, the same query prefix and the
// same name-only text the index would embed.
//
// Name-only on purpose: embedding the docstring and source too (as
// buildEmbeddingText does in production) would let prose answer the query and
// hide whether the identifier itself was reachable — the same confound
// TestAbbreviatedIdentifierRecall isolates.
func TestSemanticReachOfAbbreviations(t *testing.T) {
	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		if strings.Contains(err.Error(), "API version") {
			t.Fatalf("ONNX Runtime rejects the API version the binding requires — "+
				"Makefile ORT_VERSION is out of step with go.mod onnxruntime_go: %v", err)
		}
		t.Skipf("embedding client unavailable: %v", err)
	}

	ctx := context.Background()
	names := make([]string, 0, len(abbrevCorpus()))
	for _, e := range abbrevCorpusNamesOnly() {
		names = append(names, e.name)
	}

	vecs, err := client.EmbedBatch(ctx, names)
	if err != nil {
		t.Skipf("embedding unavailable (model/runtime not loadable): %v", err)
	}
	if len(vecs) != len(names) {
		t.Fatalf("embedder returned %d vectors for %d names", len(vecs), len(names))
	}

	qe, ok := client.(ai.QueryEmbedder)
	if !ok {
		t.Skip("client does not implement QueryEmbedder")
	}

	for _, query := range []string{"config", "configuration"} {
		qv, err := qe.EmbedQuery(ctx, query)
		if err != nil {
			t.Fatalf("embed query %q: %v", query, err)
		}

		type scored struct {
			name string
			sim  float64
		}
		ranked := make([]scored, 0, len(names))
		for i, n := range names {
			ranked = append(ranked, scored{n, cosine(qv, vecs[i])})
		}
		sort.Slice(ranked, func(i, j int) bool { return ranked[i].sim > ranked[j].sim })

		t.Logf("=== query %q (model %s) ===", query, client.ModelName())
		for _, r := range ranked {
			t.Logf("  %-20s %.4f", r.name, r.sim)
		}

		unrelated := map[string]bool{"computeChecksum": true, "PKG_ACCOUNT_UPDATE": true}
		worstRelated, bestUnrelated := math.Inf(1), math.Inf(-1)
		var cfgLoadRank int
		cfgLoadSim := math.Inf(-1)
		for i, r := range ranked {
			if r.name == "CFG_LOAD" {
				cfgLoadRank, cfgLoadSim = i+1, r.sim
			}
			if unrelated[r.name] {
				bestUnrelated = math.Max(bestUnrelated, r.sim)
			} else {
				worstRelated = math.Min(worstRelated, r.sim)
			}
		}
		t.Logf("  -> CFG_LOAD ranked %d/%d; worst config-related %.4f vs best unrelated %.4f (separated: %v)",
			cfgLoadRank, len(ranked), worstRelated, bestUnrelated, worstRelated > bestUnrelated)

		if worstRelated <= bestUnrelated {
			t.Errorf("query %q does not separate config-related identifiers from unrelated ones "+
				"(worst related %.4f <= best unrelated %.4f) — the semantic path no longer covers "+
				"abbreviations and the expansion-field idea would need revisiting",
				query, worstRelated, bestUnrelated)
		}
		if cfgLoadRank == 0 {
			t.Errorf("query %q did not rank CFG_LOAD at all", query)
		} else if cfgLoadSim <= bestUnrelated {
			t.Errorf("query %q ranked CFG_LOAD (%.4f, position %d/%d) at or below an unrelated identifier "+
				"(%.4f) — the semantic pass no longer covers the one identifier trigrams cannot reach",
				query, cfgLoadSim, cfgLoadRank, len(ranked), bestUnrelated)
		}
	}
}
