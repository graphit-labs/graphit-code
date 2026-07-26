package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// TestSearchIndexDiagnostic narrows down where a lexical query is lost.
//
// Three hypotheses were tested and refuted in order — bulk insert skipping FTS
// maintenance, an index created before its rows, and a bound parameter matching
// nothing — so rather than guess a fourth, this walks the real implementation and
// reports, at each layer, what the same database answers:
//
//	rows present -> raw interpolated FTS query -> the wrapper's own pass -> Search()
//
// Whichever layer first returns nothing is the defect.
func TestSearchIndexDiagnostic(t *testing.T) {
	dir := t.TempDir()
	corpus := gateCorpus()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)
	lb := buildSearchIndex(t, dir, cache, nil)

	count := func(label, cypher string) {
		res, err := lb.conn.Query(cypher)
		if err != nil {
			t.Logf("  %-34s ERROR %v", label, err)
			return
		}
		defer res.Close()
		var vals []string
		for res.HasNext() {
			tup, e := res.Next()
			if e != nil {
				break
			}
			var cells []string
			for i := uint64(0); i < 3; i++ {
				v, err := tup.GetValue(i)
				if err != nil {
					break
				}
				cells = append(cells, fmt.Sprint(v))
			}
			vals = append(vals, "["+fmt.Sprint(cells)+"]")
			if len(vals) >= 5 {
				break
			}
		}
		t.Logf("  %-34s %v", label, vals)
	}

	t.Log("layer 0 — files on disk (the FTS extension may keep index data outside the main file):")
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("read dir: %v", err)
	}
	for _, e := range entries {
		info, _ := e.Info()
		size := int64(-1)
		if info != nil {
			size = info.Size()
		}
		t.Logf("  %-28s dir=%v size=%d", e.Name(), e.IsDir(), size)
	}

	t.Log("layer 1 — rows actually stored:")
	count("count entities", "MATCH (e:SearchEntity) RETURN count(e)")
	count("count files", "MATCH (f:SearchFile) RETURN count(f)")
	count("a stored name_split", "MATCH (e:SearchEntity {uid:'u9'}) RETURN e.name, e.name_split, e.name_tri")

	t.Log("layer 2 — raw interpolated FTS query on the live connection:")
	count("se_split 'checksum'",
		"CALL QUERY_FTS_INDEX('SearchEntity','se_split','checksum') RETURN node.name, score")
	count("se_split 'computeChecksum'",
		"CALL QUERY_FTS_INDEX('SearchEntity','se_split','computeChecksum') RETURN node.name, score")
	count("se_doc 'checksum'",
		"CALL QUERY_FTS_INDEX('SearchEntity','se_doc','checksum') RETURN node.name, score")
	count("sf_name 'hash'",
		"CALL QUERY_FTS_INDEX('SearchFile','sf_name','hash') RETURN node.name, score")

	t.Log("layer 3 — the wrapper's own pass:")
	scores := make(map[string]*rankedSearchDoc)
	lb.ftsPass("SearchEntity", "se_split", "checksum", 1.0, 10, scores, false)
	t.Logf("  ftsPass se_split 'checksum' -> %d docs", len(scores))
	for k, v := range scores {
		t.Logf("    %s score=%.5f name=%q", k, v.score, v.result.Name)
	}

	t.Log("layer 4 — Search():")
	res, err := lb.Search("checksum", 5)
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	t.Logf("  Search(\"checksum\") -> %d results", len(res))
	for _, r := range res {
		t.Logf("    %s %s %s", r.Type, r.Path, r.Name)
	}

	if len(res) == 0 {
		t.Error("Search returned nothing — see which layer above went empty first")
	}

	// layer 5 — the same writes WITHOUT the build-and-swap cycle. RebuildFromCache
	// builds into a sibling path and renames; if this path works and that one does not,
	// the swap is the defect rather than the write.
	t.Log("layer 5 — direct write into a live index, no swap:")
	direct, err := OpenSearchIndex(filepath.Join(dir, "direct"))
	if err != nil {
		t.Fatalf("open direct index: %v", err)
	}
	defer func() { _ = direct.Close() }()

	rows := []map[string]any{{
		"uid": "d1", "name": "computeChecksum",
		"name_split": "computeChecksum compute Checksum",
		"name_tri":   identifierTrigrams("computeChecksum"),
		"docstring":  "Computes a checksum of the payload.",
		"etype":      "Function", "path": "hash.go", "line": int64(1),
	}}
	if err := direct.flushBatch(insertEntityCypher, rows); err != nil {
		t.Fatalf("direct insert: %v", err)
	}

	dres, err := direct.conn.Query(
		"CALL QUERY_FTS_INDEX('SearchEntity','se_split','checksum') RETURN node.name, score")
	if err != nil {
		t.Fatalf("direct fts query: %v", err)
	}
	n := 0
	for dres.HasNext() {
		if tup, e := dres.Next(); e == nil {
			if v, e2 := tup.GetValue(0); e2 == nil {
				t.Logf("    hit: %v", v)
				n++
			}
		}
	}
	dres.Close()
	t.Logf("  direct write, se_split 'checksum' -> %d hits", n)
	if n == 0 {
		t.Error("even a direct write into a live index is invisible to FTS — the defect is in the " +
			"schema or the write, not in the build-and-swap cycle")
	}
}
