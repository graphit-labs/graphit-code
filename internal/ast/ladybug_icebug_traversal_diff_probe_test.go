package ast

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"sort"
	"testing"
	"time"
)

type traversalProbeCase struct {
	Name   string `json:"name"`
	Cypher string `json:"cypher"`
	Rows   int    `json:"rows"`
	Order  string `json:"order_hash"`
	Set    string `json:"set_hash"`
	Err    string `json:"err,omitempty"`
	Millis int64  `json:"ms"`
}

func TestTraversalDiffProbe(t *testing.T) {
	dir := os.Getenv("GRAPHIT_REAL_BUNDLE")
	dest := os.Getenv("GRAPHIT_PROBE_OUT")
	if dir == "" || dest == "" {
		t.Skip("set GRAPHIT_REAL_BUNDLE and GRAPHIT_PROBE_OUT")
	}
	db := NewLadybugDBReadOnly(LadybugConfig{StoreDir: dir, IcebugDir: dir})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	anchors := []string{
		"internal/ast/direct_icebug.go::exportDirectWithReverse",
		"internal/ast/rebuild_index.go::newRebuildIndex",
		"internal/ast/ladybug.go::runQuery",
		"internal/ast/pipeline.go::Run",
		"internal/ast/shard_cache.go::StreamEntries",
	}
	var cases []traversalProbeCase
	add := func(name, cypher string) { cases = append(cases, traversalProbeCase{Name: name, Cypher: cypher}) }

	for i, uid := range anchors {
		add(fmt.Sprintf("a%d.1hop.callees.uid", i), fmt.Sprintf(
			"MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", uid))
		add(fmt.Sprintf("a%d.1hop.callers.uid", i), fmt.Sprintf(
			"MATCH (a:Function)-[:CALLS]->(b:Function) WHERE b.uid IN ['%s'] RETURN DISTINCT a.uid", uid))
		add(fmt.Sprintf("a%d.1hop.callers.props", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS]->(b) WHERE b.uid IN ['%s'] RETURN DISTINCT a.name", uid))
		add(fmt.Sprintf("a%d.1hop.callees.props", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS]->(b) WHERE a.uid IN ['%s'] RETURN DISTINCT b.name", uid))
		add(fmt.Sprintf("a%d.2hop.callers.uid", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS*1..2]->(b) WHERE b.uid IN ['%s'] RETURN DISTINCT a.uid", uid))
		add(fmt.Sprintf("a%d.3hop.callers.uid", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS*1..3]->(b) WHERE b.uid IN ['%s'] RETURN DISTINCT a.uid", uid))
		add(fmt.Sprintf("a%d.3hop.callers.props", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS*1..3]->(b) WHERE b.uid IN ['%s'] RETURN DISTINCT a.name", uid))
		add(fmt.Sprintf("a%d.2to3hop.callers.uid", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS*2..3]->(b) WHERE b.uid IN ['%s'] RETURN DISTINCT a.uid", uid))
		add(fmt.Sprintf("a%d.unbounded.callers.uid", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS*]->(b) WHERE b.uid IN ['%s'] RETURN DISTINCT a.uid", uid))
		add(fmt.Sprintf("a%d.directionless.uid", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS]-(b) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", uid))
		add(fmt.Sprintf("a%d.count.callers", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS]->(b) WHERE b.uid IN ['%s'] RETURN count(a.uid)", uid))
		add(fmt.Sprintf("a%d.countdistinct.callers", i), fmt.Sprintf(
			"MATCH (a)-[:CALLS]->(b) WHERE b.uid IN ['%s'] RETURN count(DISTINCT a.uid)", uid))
		add(fmt.Sprintf("a%d.reads_field", i), fmt.Sprintf(
			"MATCH (a)-[:READS_FIELD]->(b) WHERE a.uid IN ['%s'] RETURN DISTINCT b.uid", uid))
		add(fmt.Sprintf("a%d.has_parameter", i), fmt.Sprintf(
			"MATCH (a)-[:HAS_PARAMETER]->(p) WHERE a.uid IN ['%s'] RETURN DISTINCT p.name", uid))
	}
	for _, f := range []string{"internal/ast/direct_icebug.go", "internal/ast/pipeline.go", "internal/ast/ladybug.go"} {
		add("contains."+f, fmt.Sprintf(
			"MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path IN ['%s'] RETURN DISTINCT e.uid", f))
		add("contains.props."+f, fmt.Sprintf(
			"MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path IN ['%s'] RETURN DISTINCT e.name", f))
		add("imports."+f, fmt.Sprintf(
			"MATCH (f:File)-[:IMPORTS]->(m) WHERE f.path IN ['%s'] RETURN DISTINCT m.name", f))
	}

	for i := range cases {
		start := time.Now()
		res, err := db.Query(context.Background(), cases[i].Cypher, nil)
		cases[i].Millis = time.Since(start).Milliseconds()
		if err != nil {
			cases[i].Err = err.Error()
			continue
		}
		lines := make([]string, 0, len(res.Records))
		for _, r := range res.Records {
			lines = append(lines, icebugRecordKey(r))
		}
		cases[i].Rows = len(lines)
		cases[i].Order = hashProbeLines(lines)
		sorted := append([]string(nil), lines...)
		sort.Strings(sorted)
		cases[i].Set = hashProbeLines(sorted)
	}

	var total int64
	for _, c := range cases {
		total += c.Millis
	}
	t.Logf("%d queries, total %d ms", len(cases), total)

	raw, _ := json.MarshalIndent(cases, "", " ")
	if err := os.WriteFile(dest, raw, 0o644); err != nil {
		t.Fatal(err)
	}
}

func hashProbeLines(lines []string) string {
	h := sha256.New()
	for _, l := range lines {
		fmt.Fprintf(h, "%s\n", l)
	}
	return hex.EncodeToString(h.Sum(nil))[:16]
}
