//go:build lancedb

package ast

import (
	"testing"
)

func TestTrigramNoiseDoesNotDisplaceExactMatches(t *testing.T) {
	si := buildSearchIndexFrom(t, t.TempDir(), gateCorpus())

	cases := []struct{ query, wantTop string }{
		{"parseConfig", "parseConfig"},
		{"checksum", "computeChecksum"},
		{"validateSchema", "validateSchema"},
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
