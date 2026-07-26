package ast

import (
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// Pure query- and identifier-processing helpers shared by the search index.
//
// They live apart from any engine because they encode decisions that were measured
// rather than chosen, and those decisions outlive the storage underneath them:
// identifier splitting is mandatory for "config" to reach parseConfig at all
// (TestCamelCaseSearchStrategies), trigram bags are mandatory for it to reach coreConf
// (TestAbbreviatedIdentifierSearch), and the deterministic sort exists because ranking
// otherwise depended on index insertion order (TestSearchOrderIsDeterministic).
//
// Extracted when the SQLite (FTS5 + sqlite-vec) index was removed in favour of LadybugDB.

const rrfK = 60

// ---------------------------------------------------------------------------
// Deterministic ordering
// ---------------------------------------------------------------------------
// sortResultsDeterministic orders results by descending relevance with a total order on
// ties.
//
// Without it, ranking depends on the order rows were written into the index. The engine
// breaks equal BM25 scores by physical row order, and a rebuild inserts while iterating the
// parse cache — a map — so two builds of the same corpus produce different rank POSITIONS,
// which the RRF fusion turns into different scores rather than merely a different tie
// order. Measured before the fix: the query "valid" returned PKG_VALIDACAO_PAGAMENTO or
// validateSchema at top-1 depending on the build (TestSearchOrderIsDeterministic).
//
// Applied to each pass before fusion, not only to the fused list, because the pass rank
// position is what feeds the RRF score.
//
// Residual nondeterminism, deliberately not addressed: WHICH tied rows fall inside a pass's
// result limit is still the engine's choice, so a tie exactly at that boundary can vary.
// Pinning that too would mean forcing a secondary sort key inside the engine's ranked scan,
// trading its top-N path on every query for the stability of the least significant row of
// an over-fetched window.
//
// Found on the SQLite (FTS5) index and kept when it was replaced: the exposure is in the
// fusion, not in the storage, so it survived the migration unchanged.
func sortResultsDeterministic(results []SearchResult) {
	sort.Slice(results, func(i, j int) bool {
		if results[i].RelevanceScore != results[j].RelevanceScore {
			return results[i].RelevanceScore > results[j].RelevanceScore
		}
		return deduplicationKey(results[i]) < deduplicationKey(results[j])
	})
}
func deduplicationKey(r SearchResult) string {
	return r.Path + "\x00" + r.Name + "\x00" + fmt.Sprintf("%d", r.Line)
}

// ---------------------------------------------------------------------------
// Query tokenisation
// ---------------------------------------------------------------------------
var reservedQueryWords = map[string]bool{
	"AND": true, "OR": true, "NOT": true, "NEAR": true,
}

func tokenizeQuery(query string) []string {
	raw := strings.Fields(query)
	tokens := make([]string, 0, len(raw)*2)
	for _, t := range raw {
		t = stripFTSSpecialChars(t)
		if t == "" || reservedQueryWords[strings.ToUpper(t)] {
			continue
		}
		tokens = append(tokens, t)
		if splits := splitCodeIdentifier(t); splits != t {
			for _, s := range strings.Fields(splits) {
				s = stripFTSSpecialChars(s)
				if s != "" && !reservedQueryWords[strings.ToUpper(s)] {
					tokens = append(tokens, s)
				}
			}
		}
	}
	return dedupTokens(tokens)
}
func stripFTSSpecialChars(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		if r == '"' || r == '*' || r == '^' || r == '(' || r == ')' || r == '{' || r == '}' || r == ':' {
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

// ---------------------------------------------------------------------------
// Trigram bags
// ---------------------------------------------------------------------------
// normalizeForTrigrams folds an identifier to the alphabet trigrams are built
// over: lowercase alphanumerics, separators dropped. Dropping rather than
// splitting on separators keeps substrings that straddle a word boundary
// findable — "reconf" still occurs in coreconf.
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

// identifierTrigrams renders an identifier as the space-separated bag of its
// overlapping 3-grams ("coreConf" -> "cor ore rec eco con onf"), which a word
// tokenizer then indexes as ordinary terms. Inputs too short to yield a trigram
// are emitted whole so they are at least present in the column.
func identifierTrigrams(name string) string {
	norm := []rune(normalizeForTrigrams(name))
	if len(norm) < 3 {
		return string(norm)
	}
	grams := make([]string, 0, len(norm)-2)
	for i := 0; i+3 <= len(norm); i++ {
		grams = append(grams, string(norm[i:i+3]))
	}
	return strings.Join(grams, " ")
}

// ---------------------------------------------------------------------------
// Code identifier splitting
// ---------------------------------------------------------------------------
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
