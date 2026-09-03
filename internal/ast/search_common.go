package ast

import (
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// This file holds what the search engine decides regardless of where the index is
// stored: how a query is tokenised, how an identifier is split and folded to the gram
// alphabet, which semantic neighbours are confident enough to vote, and how ties are
// broken.
//
// Each of these is a measured decision rather than a chosen one. FUSION IS NOT AMONG
// THEM any more: ranking belongs to the engine, and the Go-side RRF constant and trigram
// bag that used to live here went out with it — see
// Graphit Task tsk-5338838de9c4.

// Similarity a semantic neighbour must reach to take part at all.
//
// The floor exists because hybrid search was measurably WORSE than lexical: 9 of 16 probes
// against 13, losing four and gaining none. Nearest-neighbour search always returns
// neighbours — for a two-character query the embedding carries no meaning, yet the pass
// contributed anyway and drowned exact matches. Both "cf" and "audit" came back as
// computeChecksum. With the floor, both are correct again and hybrid reaches parity with
// lexical on every probe that has a defensible answer.
//
// The floor is read off the separation the model actually produces: on identifier text,
// related entities scored 0.34-0.39 cosine and unrelated ones 0.07-0.08
// (TestSemanticReachOfAbbreviations). A neighbour below the floor is not evidence of
// anything, so it does not vote.
const semanticFloorCosine = 0.20

// cosineFromSquaredL2 converts the engine's vector distance into the cosine similarity the
// floor above is expressed in.
//
// It is needed because a PURE VECTOR QUERY CARRIES NO SCORE. The engine returns `_distance` and
// neither `_score` nor `_relevance_score`, so RelevanceScore arrived as zero on every semantic
// result — and zero is below the floor, so confidentSemanticResults truncated at the first row
// and SemanticSearch returned NOTHING, for every query, on every corpus. It had been that way
// since the port off SQLite, where the cosine was computed in Go and written into that field.
//
// MEASURED against unit vectors whose cosine to the query is known exactly
// (TestVectorMetricIsSquaredL2OnUnitVectors):
//
//	cosine 1.000 -> distance 0.000
//	cosine 0.707 -> distance 0.586
//	cosine 0.500 -> distance 1.000
//	cosine 0.000 -> distance 2.000
//
// which is d = 2 - 2cos, so cos = 1 - d/2. Exact rather than approximate, because the embedder
// L2-normalises every vector it returns — see the batch loop in internal/ai/embedding_local.go.
// If that normalisation ever goes, this conversion goes with it.
func cosineFromSquaredL2(distance float64) float64 { return 1 - distance/2 }

// confidentSemanticResults drops neighbours too dissimilar to mean anything. Results arrive
// ordered by distance, so this truncates at the first one below the floor.
func confidentSemanticResults(results []SearchResult) []SearchResult {
	for i, r := range results {
		if r.RelevanceScore < semanticFloorCosine {
			return results[:i]
		}
	}
	return results
}

// tokenizeQuery reduces a raw query to the terms the index is searched with, adding the
// split form of every identifier-shaped token.
//
// The split is not an optimisation: without it "config" does not reach parseConfig at all
// (measured 0/6 -> 6/6), because an exact-token index has no way to see the word inside a
// camelCase name.
func tokenizeQuery(query string) []string {
	raw := strings.Fields(query)
	tokens := make([]string, 0, len(raw)*2)
	for _, t := range raw {
		t = stripQuerySpecialChars(t)
		if t == "" || reservedWords[strings.ToUpper(t)] {
			continue
		}
		tokens = append(tokens, t)
		if splits := splitCodeIdentifier(t); splits != t {
			for _, s := range strings.Fields(splits) {
				s = stripQuerySpecialChars(s)
				if s != "" && !reservedWords[strings.ToUpper(s)] {
					tokens = append(tokens, s)
				}
			}
		}
	}
	return dedupTokens(tokens)
}

// reservedWords are dropped from a query because a full-text parser reads them as syntax
// rather than as words.
//
// Kept from the FTS5 era even though LadybugDB's query language is simpler: none of them is
// a useful search term on its own, and letting one reach the engine's parser turns a search
// into a syntax error rather than a bad result.
var reservedWords = map[string]bool{
	"AND": true, "OR": true, "NOT": true, "NEAR": true,
}

// stripQuerySpecialChars removes characters a full-text query language would read as
// syntax, plus control characters.
//
// The set is deliberately kept from the FTS5 era even though LadybugDB's query parser is
// simpler: these characters carry no meaning in an identifier search either way, and a
// narrower filter would only widen the surface where a user's punctuation reaches the
// engine's parser.
func stripQuerySpecialChars(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		switch r {
		case '"', '*', '^', '(', ')', '{', '}', ':', '\'', '-':
			continue
		}
		if unicode.IsControl(r) {
			continue
		}
		b.WriteRune(r)
	}
	return strings.TrimSpace(b.String())
}

func dedupTokens(tokens []string) []string {
	seen := make(map[string]bool, len(tokens))
	out := make([]string, 0, len(tokens))
	for _, t := range tokens {
		low := strings.ToLower(t)
		if !seen[low] {
			seen[low] = true
			out = append(out, t)
		}
	}
	return out
}

func deduplicationKey(r SearchResult) string {
	return r.Path + "\x00" + r.Name + "\x00" + fmt.Sprintf("%d", r.Line)
}

// sortResultsDeterministic orders results by descending relevance with a total order
// on ties.
//
// Without it, ranking depends on the order rows were inserted: the engine breaks equal
// BM25 scores by internal row order, and a rebuild inserts while iterating a map, so two
// builds of the same corpus produce different rank POSITIONS — which the RRF fusion turns
// into different scores, not merely a different tie order. Measured before the fix: the
// query "valid" returned PKG_VALIDACAO_PAGAMENTO or validateSchema at top-1 depending on
// the build (TestSearchOrderIsDeterministic).
//
// Applied to each pass before fusion, not only to the fused list, because the pass rank
// position is what feeds the RRF score.
//
// Residual nondeterminism, deliberately not addressed: WHICH tied rows fall inside a
// pass's LIMIT window is still the engine's choice, so a tie exactly at the fetch boundary
// can vary. Forcing that too would mean a secondary ORDER BY in the query, giving up the
// engine's top-N path on every search to stabilise the least significant row of an
// over-fetched window.
func sortResultsDeterministic(results []SearchResult) {
	sort.Slice(results, func(i, j int) bool {
		if results[i].RelevanceScore != results[j].RelevanceScore {
			return results[i].RelevanceScore > results[j].RelevanceScore
		}
		return deduplicationKey(results[i]) < deduplicationKey(results[j])
	})
}

// normalizeForTrigrams folds an identifier to the alphabet trigrams are built over:
// lowercase alphanumerics, separators dropped. Dropping rather than splitting on
// separators keeps substrings that straddle a word boundary findable — "reconf" still
// occurs in coreconf.
func normalizeForTrigrams(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range strings.ToLower(s) {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// splitCodeIdentifier splits camelCase, PascalCase, snake_case, and dot.notation
// identifiers into space-separated tokens. Returns the original string unchanged
// if no splitting occurred.
func splitCodeIdentifier(name string) string {
	if name == "" {
		return name
	}

	normalized := strings.NewReplacer("_", " ", ".", " ", "-", " ").Replace(name)

	var parts []string
	for _, word := range strings.Fields(normalized) {
		parts = append(parts, splitCamelCase(word)...)
	}

	result := strings.Join(parts, " ")
	if result == name {
		return name
	}
	return result
}

func splitCamelCase(s string) []string {
	if s == "" {
		return nil
	}

	runes := []rune(s)
	var parts []string
	start := 0

	for i := 1; i < len(runes); i++ {
		prevUpper := unicode.IsUpper(runes[i-1])
		currUpper := unicode.IsUpper(runes[i])
		currLower := unicode.IsLower(runes[i])

		if !prevUpper && currUpper {
			parts = append(parts, string(runes[start:i]))
			start = i
		} else if prevUpper && currLower && i-start > 1 {
			parts = append(parts, string(runes[start:i-1]))
			start = i - 1
		}
	}
	parts = append(parts, string(runes[start:]))

	var filtered []string
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			filtered = append(filtered, p)
		}
	}
	return filtered
}
