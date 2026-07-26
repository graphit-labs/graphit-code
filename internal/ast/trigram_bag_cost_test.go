package ast

import (
	"testing"
)

// The trigram recall pass buys reach for abbreviated identifiers
// (TestAbbreviationRecallByNameAlone) by OR-ing the query's trigrams instead of
// requiring them as a phrase. That trade has two costs worth bounding, and both
// are measured here rather than argued:
//
//  1. precision — unrelated names sharing a common gram now enter the result set
//     ("checksum" and "validateSchema" share "che"). Acceptable only while the
//     noise stays BELOW real matches, which is what the RRF weight (0.7 versus
//     1.5–3.0 for the exact passes) is supposed to guarantee;
//  2. latency — an OR over ~6 grams matches far more rows than a phrase would, and
//     ranking forces BM25 over every candidate. Measured in TestSearchIndexScaleCost.

// TestTrigramNoiseDoesNotDisplaceExactMatches is the precision invariant. Noise
// ranked beneath a true hit is a cost; noise that outranks it is a regression.
func TestTrigramNoiseDoesNotDisplaceExactMatches(t *testing.T) {
	si := buildSearchIndexFrom(t, t.TempDir(), gateCorpus())

	// Every probe whose query names an entity outright, plus the abbreviation-era
	// queries that share grams with unrelated entities in this corpus.
	cases := []struct{ query, wantTop string }{
		{"parseConfig", "parseConfig"},
		{"checksum", "computeChecksum"},      // shares "che" with validateSchema
		{"validateSchema", "validateSchema"}, // shares "che" with computeChecksum
		{"retryPolicy", "retryPolicy"},
		{"parseSQL", "parseSQL"},
		{"connectDatabase", "connectDatabase"},
	}

	for _, cs := range cases {
		got := indexSearchNames(t, si, cs.query, 5)
		if len(got) == 0 {
			t.Errorf("query %q returned nothing", cs.query)
			continue
		}
		t.Logf("%-16s -> %v", cs.query, got)
		if got[0] != cs.wantTop {
			t.Errorf("trigram noise displaced the exact match for %q: top-1 is %q, want %q (full: %v)",
				cs.query, got[0], cs.wantTop, got)
		}
	}
}

// The latency half of this file moved to TestSearchIndexScaleCost, which measures the same
// thing on the real index at the same 200k scale and also covers the incremental path — the
// number that actually decides whether the design is affordable.
