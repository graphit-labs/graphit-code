package ast

import (
	"fmt"
	"sort"
	"strings"
	"unicode"
)

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

func cosineFromSquaredL2(distance float64) float64 { return 1 - distance/2 }

func confidentSemanticResults(results []SearchResult) []SearchResult {
	for i, r := range results {
		if r.RelevanceScore < semanticFloorCosine {
			return results[:i]
		}
	}
	return results
}

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

var reservedWords = map[string]bool{
	"AND": true, "OR": true, "NOT": true, "NEAR": true,
}

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

func sortResultsDeterministic(results []SearchResult) {
	sort.Slice(results, func(i, j int) bool {
		if results[i].RelevanceScore != results[j].RelevanceScore {
			return results[i].RelevanceScore > results[j].RelevanceScore
		}
		return deduplicationKey(results[i]) < deduplicationKey(results[j])
	})
}

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
