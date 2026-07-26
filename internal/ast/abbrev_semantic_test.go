package ast

import (
	"context"
	"math"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// Two ways to reach an abbreviated identifier from a spelled-out query, both
// measured here, because the trigram pass leaves exactly one case open: CFG_LOAD
// shares no trigram with "config" and is unreachable by any lexical method
// (TestAbbreviationRecallByNameAlone, TestAbbreviatedIdentifierSearch).
//
//   - An EXPANSION FIELD ("CFG_LOAD" -> "configuration load") indexed alongside
//     the name. Deterministic and lexical once the text exists; the open question
//     is who writes the text. TestExpansionFieldCeiling measures what it is worth
//     assuming a perfect expansion, so the payoff is known before committing to a
//     way of producing it.
//   - The EMBEDDING already computed for every entity. This needs no new field and
//     no new model: buildEmbeddingText already includes the entity name, and
//     HybridSearch already fuses BM25 with vector search by RRF.
//     TestSemanticReachOfAbbreviations measures whether that path reaches CFG_LOAD.
//
// Note what CodeRankEmbed cannot do: it maps text to 768 floats, so it cannot
// GENERATE "core configuration" from "coreCfg". Producing expansion text needs a
// generative model, which is a separate dependency — the embedder can only be used
// for the second route.

// abbrevExpansions is a hand-written ideal expansion per identifier: what a perfect
// generator would emit. Hand-written on purpose — the point is to measure the ceiling of
// the idea, not the accuracy of any particular generator.
//
// cfgLoadExpansion is varied by the test because it turns out to decide the answer.
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

// TestExpansionFieldCeiling measures the best case for an expansion field: whether it buys
// the one match trigrams cannot reach, CFG_LOAD, which shares no substring with "config".
//
// The answer moved when SQLite was removed, and the reason is worth keeping. Measured on
// the FTS5 index, a perfect expansion scored 9/9 — it reached CFG_LOAD through the
// expansion "configuration load". That worked because of the PREFIX index: the query
// "config" matched the token "configuration" as a prefix. LadybugDB has no wildcard
// operator and its porter stemmer does not reduce "configuration" to "config", so the same
// expansion now matches nothing, and the trigram field covers names only.
//
// So the expansion has to contain the query's exact token. Both wordings are measured
// below, because "an expansion field would fix this" is only true for expansions that
// happen to repeat the searcher's word — a much weaker claim than the 9/9 suggested, and
// one no generator can guarantee.
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
		t.Logf("  recall: %d/%d (trigram alone, no expansion field: 8/9)", total, wantTotal)
		results[v.label] = total

		if wantTotal == 0 {
			t.Fatal("no expectations were probed — the test measured nothing")
		}
	}

	morph := results["morphological variant (\"configuration load\")"]
	exact := results["exact query token (\"config load\")"]
	t.Logf("expansion recall: morphological %d, exact-token %d", morph, exact)

	// On this engine both wordings reach CFG_LOAD, because FTS5's prefix index lets the
	// query "config" match the token "configuration". That is exactly what made the
	// expansion field look like a 9/9 idea.
	//
	// It is engine-specific, and worth keeping written down: on an engine without prefix
	// matching the morphological wording scored 8/9 while the exact-token one scored 9/9,
	// so the expansion only helped when it happened to repeat the searcher's word. Any
	// future move off FTS5 inherits that weaker claim.
	if morph < exact {
		t.Errorf("the morphological expansion (%d) no longer matches as well as the exact-token "+
			"one (%d) — prefix matching has been lost, and an expansion field would only help "+
			"when it repeats the query's exact word", morph, exact)
	}
	if exact <= 8 {
		t.Errorf("a perfect expansion field scored %d/9, so it buys nothing over the trigram bag "+
			"alone (8/9) and should not be built", exact)
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
		// A machine without the model may legitimately skip. A runtime that
		// refuses the binding's API version is a build-configuration bug and must
		// not hide behind a skip — that is exactly how the ORT 1.25 / API 26
		// mismatch went unnoticed from 2026-07-22, silently degrading semantic
		// search to FTS-only. Makefile ORT_VERSION must track go.mod's
		// onnxruntime_go.
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

		// The question that decides whether an expansion field is needed: does the
		// embedding rank the config-related identifiers — including the one no
		// substring method reaches — above the unrelated ones?
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

		// This is the finding that makes an expansion field unnecessary, so it is
		// asserted rather than logged: every config-related identifier — including
		// the heavy abbreviation CFG_LOAD, which shares no trigram with "config" —
		// must outrank every unrelated one. Measured separation was 0.34 vs 0.08.
		if worstRelated <= bestUnrelated {
			t.Errorf("query %q does not separate config-related identifiers from unrelated ones "+
				"(worst related %.4f <= best unrelated %.4f) — the semantic path no longer covers "+
				"abbreviations and the expansion-field idea would need revisiting",
				query, worstRelated, bestUnrelated)
		}
		// CFG_LOAD gets its own assertion because it is the crux: no lexical method
		// reaches it, so the semantic pass is its only route into the candidate set.
		// The bar is outranking the noise, not being first — measured rank varies with
		// the query wording (1st for "config", 4th for "configuration") while the
		// margin over unrelated entities stays roughly 4x. Demanding a fixed rank here
		// would encode one wording's luck as a requirement.
		if cfgLoadRank == 0 {
			t.Errorf("query %q did not rank CFG_LOAD at all", query)
		} else if cfgLoadSim <= bestUnrelated {
			t.Errorf("query %q ranked CFG_LOAD (%.4f, position %d/%d) at or below an unrelated identifier "+
				"(%.4f) — the semantic pass no longer covers the one identifier trigrams cannot reach",
				query, cfgLoadSim, cfgLoadRank, len(ranked), bestUnrelated)
		}
	}
}
