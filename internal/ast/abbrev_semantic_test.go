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
		t.Logf("  recall: %d/%d (without the prefix index this wording reaches 8/9)", total, wantTotal)
		results[v.label] = total

		if wantTotal == 0 {
			t.Fatal("no expectations were probed — the test measured nothing")
		}
	}

	morph := results["morphological variant (\"configuration load\")"]
	exact := results["exact query token (\"config load\")"]
	t.Logf("expansion recall: morphological %d, exact-token %d", morph, exact)

	// The two wordings score ALIKE, and that parity is the whole finding.
	//
	// This assertion has now been measured on both engines, and the pair is worth more than
	// either number, because it separates a property of the index from a property of the
	// idea:
	//
	//   - On FTS5 both wordings reach 9/9. The prefix index lets the query "config" match
	//     the token "configuration" directly, so a generated expansion field would add a
	//     column that reproduces what the index already does.
	//   - On LadybugDB, which has neither prefix matching nor a wildcard, the morphological
	//     wording dropped to 8/9 — level with the trigram bag alone — and only the wording
	//     that happened to repeat the searcher's exact word still reached 9/9.
	//
	// So on the engine that ships, an expansion field buys nothing; on the one that does
	// not, it bought a single probe, and only when its author guessed the searcher's exact
	// word. No generator guarantees that. The field is not worth building, and the reason
	// no longer depends on which storage engine is underneath.
	//
	// WHAT THIS GUARDS NOW IS THE CONCLUSION, NOT THE MECHANISM. It used to guard the prefix
	// index, and the prefix index is gone with SQLite — LanceDB's BM25 has no wildcard operator,
	// so the gram bag carries every truncation. Asserting a mechanism that no longer exists would
	// make this test fail for the one reason that is not a regression.
	//
	// The finding it exists to protect is unchanged: an expansion field is not worth building,
	// because it pays only when its author happened to write the searcher's exact word, and no
	// generator can guarantee that. Two things say so, and both are still measurable.
	//
	// MEASURED here: exact-token 9/9, morphological 8/9. The single probe of difference IS the
	// lucky guess, quantified.
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
