package ast

import (
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// tokenizeQuery
// ---------------------------------------------------------------------------

func TestTokenizeQuery(t *testing.T) {
	tests := []struct {
		name  string
		query string
		check func(t *testing.T, tokens []string)
	}{
		{
			name:  "simple_single",
			query: "hello",
			check: func(t *testing.T, tokens []string) {
				assertContains(t, tokens, "hello")
			},
		},
		{
			name:  "multiple_words",
			query: "foo bar",
			check: func(t *testing.T, tokens []string) {
				assertContains(t, tokens, "foo")
				assertContains(t, tokens, "bar")
			},
		},
		{
			name:  "filters_reserved_words",
			query: "AND OR NOT foo",
			check: func(t *testing.T, tokens []string) {
				assertNotContains(t, tokens, "AND")
				assertNotContains(t, tokens, "OR")
				assertNotContains(t, tokens, "NOT")
				assertContains(t, tokens, "foo")
			},
		},
		{
			name:  "strips_special_chars",
			query: `"hello" *world* (test)`,
			check: func(t *testing.T, tokens []string) {
				assertContains(t, tokens, "hello")
				assertContains(t, tokens, "world")
				assertContains(t, tokens, "test")
			},
		},
		{
			name:  "splits_camelCase",
			query: "myFunction",
			check: func(t *testing.T, tokens []string) {
				assertContains(t, tokens, "myFunction")
				// Should also have split parts
				if len(tokens) < 2 {
					t.Errorf("expected camelCase split, got %v", tokens)
				}
			},
		},
		{
			name:  "empty_query",
			query: "",
			check: func(t *testing.T, tokens []string) {
				if len(tokens) != 0 {
					t.Errorf("expected empty tokens, got %v", tokens)
				}
			},
		},
		{
			name:  "all_reserved",
			query: "AND OR NOT NEAR",
			check: func(t *testing.T, tokens []string) {
				if len(tokens) != 0 {
					t.Errorf("expected all reserved words filtered, got %v", tokens)
				}
			},
		},
		{
			name:  "dedup_tokens",
			query: "test test TEST",
			check: func(t *testing.T, tokens []string) {
				count := 0
				for _, tok := range tokens {
					if strings.EqualFold(tok, "test") {
						count++
					}
				}
				if count != 1 {
					t.Errorf("expected deduped tokens, got %v", tokens)
				}
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tokens := tokenizeQuery(tt.query)
			tt.check(t, tokens)
		})
	}
}

// ---------------------------------------------------------------------------
// stripFTSSpecialChars
// ---------------------------------------------------------------------------

func TestStripFTSSpecialChars(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{`hello`, `hello`},
		{`"quoted"`, `quoted`},
		{`func*`, `func`},
		{`(expr)`, `expr`},
		{`{block}`, `block`},
		{`a:b`, `ab`},
		{`no^caret`, `nocaret`},
		{`clean`, `clean`},
		{``, ``},
		{`"*^(){}:"`, ``},
	}
	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := stripFTSSpecialChars(tt.input)
			if got != tt.want {
				t.Errorf("stripFTSSpecialChars(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// quoteToken
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// buildPhraseQuery
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// buildANDQuery
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// buildORQuery
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// buildPrefixQuery
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// buildSearchPasses
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// deduplicationKey
// ---------------------------------------------------------------------------

func TestDeduplicationKey(t *testing.T) {
	r1 := SearchResult{Path: "a.go", Name: "Func", Line: 10}
	r2 := SearchResult{Path: "a.go", Name: "Func", Line: 10}
	r3 := SearchResult{Path: "a.go", Name: "Func", Line: 20}

	k1 := deduplicationKey(r1)
	k2 := deduplicationKey(r2)
	k3 := deduplicationKey(r3)

	if k1 != k2 {
		t.Errorf("same result should produce same key: %q vs %q", k1, k2)
	}
	if k1 == k3 {
		t.Errorf("different line should produce different key: %q vs %q", k1, k3)
	}
}

// ---------------------------------------------------------------------------
// test helpers
// ---------------------------------------------------------------------------

func assertContains(t *testing.T, tokens []string, expected string) {
	t.Helper()
	for _, tok := range tokens {
		if tok == expected {
			return
		}
	}
	t.Errorf("expected tokens %v to contain %q", tokens, expected)
}

func assertNotContains(t *testing.T, tokens []string, unexpected string) {
	t.Helper()
	for _, tok := range tokens {
		if tok == unexpected {
			t.Errorf("expected tokens %v to NOT contain %q", tokens, unexpected)
			return
		}
	}
}

// The FTS5 query-syntax builders that used to live here — quoteToken, buildPhraseQuery,
// buildANDQuery, buildORQuery, buildPrefixQuery and buildSearchPasses — were removed with
// the SQLite index. LadybugDB's FTS has no phrase, explicit-boolean or wildcard operators
// (measured: 'conf*' matches nothing), so a query is a bag of terms and those builders had
// nothing to build. What replaced them, and is covered elsewhere, is the weighted per-field
// pass set plus the trigram bag.
