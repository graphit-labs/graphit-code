package ast

import (
	"strings"
	"testing"
)

// The tail used to match the ORDER BY keyword without its arguments, so the arguments
// stayed inside the projection and the query was refused for the wrong reason. Parsing has
// to carry the clause into the plan instead.
func TestCanonicalTraversalParsesOrderByAndLimit(t *testing.T) {
	t.Parallel()

	plan, refusal, ok := parseCanonicalTraversal(
		"MATCH (f:File {path: 'a.go'})-[:CONTAINS]->(e:Function) " +
			"RETURN DISTINCT e.name, e.line_number ORDER BY e.line_number DESC, e.name LIMIT 5")
	if !ok {
		t.Fatalf("a traversal with ORDER BY and LIMIT was refused: %v", refusal)
	}
	if len(plan.orderBy) != 2 {
		t.Fatalf("expected two sort keys, got %d: %+v", len(plan.orderBy), plan.orderBy)
	}
	if plan.orderBy[0].column != "e.line_number" || !plan.orderBy[0].desc {
		t.Errorf("first sort key wrong: %+v", plan.orderBy[0])
	}
	if plan.orderBy[1].column != "e.name" || plan.orderBy[1].desc {
		t.Errorf("second sort key should default to ascending: %+v", plan.orderBy[1])
	}
	if !plan.hasLimit || plan.limit != 5 {
		t.Errorf("LIMIT not carried into the plan: hasLimit=%v limit=%d", plan.hasLimit, plan.limit)
	}
	if plan.returnClause != "e.name, e.line_number" {
		t.Errorf("the ORDER BY leaked into the projection: %q", plan.returnClause)
	}
}

// An alias is the name the record actually carries, so ordering by it must work — and
// ordering by the underlying property must resolve to the same column.
func TestCanonicalTraversalOrdersByAlias(t *testing.T) {
	t.Parallel()

	for _, term := range []string{"line", "e.line_number"} {
		plan, refusal, ok := parseCanonicalTraversal(
			"MATCH (f:File {path: 'a.go'})-[:CONTAINS]->(e:Function) " +
				"RETURN DISTINCT e.line_number AS line ORDER BY " + term)
		if !ok {
			t.Fatalf("ORDER BY %s was refused: %v", term, refusal)
		}
		if len(plan.orderBy) != 1 || plan.orderBy[0].column != "line" {
			t.Errorf("ORDER BY %s did not resolve to the aliased column: %+v", term, plan.orderBy)
		}
	}
}

// Sorting runs over the materialized rows, so a key the projection does not carry cannot be
// honoured. Refusing says so; widening the projection silently would answer a different
// query than the one that was written.
func TestCanonicalTraversalRefusesOrderByOnUnprojectedColumn(t *testing.T) {
	t.Parallel()

	_, refusal, ok := parseCanonicalTraversal(
		"MATCH (f:File {path: 'a.go'})-[:CONTAINS]->(e:Function) " +
			"RETURN DISTINCT e.name ORDER BY e.line_number")
	if ok {
		t.Fatal("ORDER BY on an unprojected column was accepted")
	}
	if refusal == nil || !strings.Contains(refusal.Error(), "does not project") {
		t.Errorf("the refusal does not name the cause: %v", refusal)
	}
}

func TestCanonicalTraversalRefusesOrderByOnACount(t *testing.T) {
	t.Parallel()

	_, refusal, ok := parseCanonicalTraversal(
		"MATCH (caller)-[:CALLS]->(f:Function {name: 'X'}) RETURN count(caller.uid) ORDER BY caller.name")
	if ok {
		t.Fatal("ORDER BY on a count was accepted")
	}
	if refusal == nil || !strings.Contains(refusal.Error(), "single row") {
		t.Errorf("the refusal does not explain that a count is one row: %v", refusal)
	}
}

// Numbers must compare numerically. A lexical sort puts line 10 before line 9, which is the
// bug an ordering feature exists to avoid.
func TestApplyCanonicalOrderingSortsNumbersNumerically(t *testing.T) {
	t.Parallel()

	records := []QueryRecord{
		{"e.name": "c", "e.line_number": int64(10)},
		{"e.name": "a", "e.line_number": int64(9)},
		{"e.name": "b", "e.line_number": int64(100)},
	}
	plan := canonicalPlan{orderBy: []canonicalOrderKey{{column: "e.line_number"}}}

	got := applyCanonicalOrdering(plan, records)
	var lines []int64
	for _, r := range got {
		lines = append(lines, r["e.line_number"].(int64))
	}
	if lines[0] != 9 || lines[1] != 10 || lines[2] != 100 {
		t.Errorf("numbers did not sort numerically: %v", lines)
	}
}

func TestApplyCanonicalOrderingHonoursDescAndLimit(t *testing.T) {
	t.Parallel()

	records := []QueryRecord{
		{"n": int64(1)}, {"n": int64(3)}, {"n": int64(2)},
	}
	plan := canonicalPlan{
		orderBy:  []canonicalOrderKey{{column: "n", desc: true}},
		limit:    2,
		hasLimit: true,
	}

	got := applyCanonicalOrdering(plan, records)
	if len(got) != 2 {
		t.Fatalf("LIMIT not applied: got %d rows", len(got))
	}
	if got[0]["n"].(int64) != 3 || got[1]["n"].(int64) != 2 {
		t.Errorf("DESC not applied before LIMIT: %v", got)
	}
}

// Without ORDER BY the order stays the reproducible canonical one, and with ORDER BY the
// canonical key remains the tiebreak — otherwise rows equal on every sort key come back in
// whatever order the batched member queries happened to produce.
func TestApplyCanonicalOrderingIsStableOnTies(t *testing.T) {
	t.Parallel()

	records := []QueryRecord{
		{"g": int64(1), "name": "z"},
		{"g": int64(1), "name": "a"},
		{"g": int64(1), "name": "m"},
	}
	plan := canonicalPlan{orderBy: []canonicalOrderKey{{column: "g"}}}

	first := applyCanonicalOrdering(plan, append([]QueryRecord{}, records...))
	second := applyCanonicalOrdering(plan, []QueryRecord{records[2], records[0], records[1]})

	for i := range first {
		if first[i]["name"] != second[i]["name"] {
			t.Fatalf("tie order is not reproducible across input orders: %v vs %v", first, second)
		}
	}
}

// A limit with no ordering is still a limit, and a query with neither must be untouched.
func TestApplyCanonicalOrderingLimitWithoutOrder(t *testing.T) {
	t.Parallel()

	records := []QueryRecord{{"n": int64(1)}, {"n": int64(2)}, {"n": int64(3)}}

	if got := applyCanonicalOrdering(canonicalPlan{limit: 2, hasLimit: true}, records); len(got) != 2 {
		t.Errorf("LIMIT without ORDER BY was ignored: %d rows", len(got))
	}
	if got := applyCanonicalOrdering(canonicalPlan{}, records); len(got) != 3 {
		t.Errorf("a plan with neither clause changed the row count: %d", len(got))
	}
}

// Every refusal the first round established must keep refusing: ordering is additive, it
// does not loosen the shape rule.
func TestOrderingDoesNotLoosenTheShapeRule(t *testing.T) {
	t.Parallel()

	cases := map[string]string{
		"label projected":   "MATCH (a)-[:CALLS]->(b:Function {name: 'X'}) RETURN DISTINCT label(a) ORDER BY label(a)",
		"missing DISTINCT":  "MATCH (a)-[:CALLS]->(b:Function {name: 'X'}) RETURN a.name ORDER BY a.name",
		"both ends":         "MATCH (a)-[:CALLS]->(b:Function {name: 'X'}) RETURN DISTINCT a.name, b.name ORDER BY a.name",
		"anchor unfiltered": "MATCH (a)-[:CALLS]->(b:Function) RETURN DISTINCT a.name ORDER BY a.name",
	}
	for name, query := range cases {
		if _, refusal, ok := parseCanonicalTraversal(query); ok || refusal == nil {
			t.Errorf("%s: adding ORDER BY made a refused shape pass", name)
		}
	}
}
