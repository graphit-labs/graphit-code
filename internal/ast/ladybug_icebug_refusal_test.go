package ast

import (
	"context"
	"strings"
	"testing"
)

func TestCanonicalRefusalNamesTheRule(t *testing.T) {
	for _, tc := range []struct {
		name, query string
		want        []string
	}{
		{
			name:  "label in the projection",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT label(e) AS type, e.name",
			want:  []string{"label is not projectable", "CALLS", "Pin the label in the pattern"},
		},
		{
			name:  "projection without DISTINCT",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN e.name",
			want:  []string{"must be DISTINCT", "RETURN DISTINCT e.name"},
		},
		{
			name:  "projection richer than a property",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT collect(e.uid)",
			want:  []string{"not a plain property projection", "collect()"},
		},
		{
			name:  "the RETURN projects the filtered end",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT f.name, e.name",
			want:  []string{"exactly one end", "both"},
		},
		{
			name:  "the RETURN projects neither end",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT 1 AS one",
			want:  []string{"exactly one end", "neither"},
		},
		{
			name:  "count over something that is not an endpoint",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN count(DISTINCT r.uid)",
			want:  []string{"exactly one end", "neither"},
		},
		{
			name:  "a predicate comparing both ends",
			query: "MATCH (f)-[:CALLS]->(e) WHERE f.name = e.name RETURN DISTINCT e.name",
			want:  []string{"compares the two ends", "one end at a time"},
		},
		{
			name:  "a predicate referencing neither end",
			query: "MATCH (f)-[:CALLS]->(e) WHERE 1 = 1 RETURN DISTINCT e.name",
			want:  []string{"references neither end"},
		},
		{
			name:  "nothing anchors the traversal",
			query: "MATCH (f)-[:CALLS]->(e) RETURN DISTINCT e.name",
			want:  []string{"nothing filters `f`", "every node of that label"},
		},
		{
			name:  "an inverted hop range",
			query: "MATCH (f)-[:CALLS*3..1]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT e.name",
			want:  []string{"*3..1 is inverted", "lower bound first"},
		},
		{
			name:  "both ends bind the same variable",
			query: "MATCH (n)-[:CALLS]->(n) WHERE n.uid IN ['fn_a'] RETURN DISTINCT n.name",
			want:  []string{"same variable"},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			_, refusal, ok := parseCanonicalTraversal(tc.query)
			if ok {
				t.Fatal("the planner accepted a query it cannot answer")
			}
			if refusal == nil {
				t.Fatal("refused without naming a rule — the caller can only guess again")
			}
			got := refusal.Error()
			for _, want := range tc.want {
				if !strings.Contains(got, want) {
					t.Errorf("refusal does not mention %q\ngot: %s", want, got)
				}
			}
			if strings.Contains(got, "multi-hop") {
				t.Errorf("refusal still blames multi-hop\ngot: %s", got)
			}
		})
	}
}

// A query that is not a traversal at all must NOT be refused: it falls through to the
// engine, which runs it on the mounted member tables exactly as written.
func TestCanonicalNonTraversalIsNotRefused(t *testing.T) {
	for _, query := range []string{
		"MATCH (n:Function) WHERE n.name = 'a' RETURN n.name, n.path",
		"MATCH (n) WHERE n.uid IN ['fn_a'] RETURN DISTINCT label(n) AS type",
	} {
		_, refusal, ok := parseCanonicalTraversal(query)
		if ok {
			t.Errorf("planned a non-traversal: %s", query)
		}
		if refusal != nil {
			t.Errorf("refused a non-traversal instead of letting the engine run it: %s\ngot: %s", query, refusal.Error())
		}
	}
}

// The rules are about what the planner can PRESERVE, so the accepted forms have to keep
// planning — a refusal message is no use if the fix it suggests is also refused.
func TestCanonicalAcceptedFormsStillPlan(t *testing.T) {
	for _, query := range []string{
		"MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT e.name",
		"MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT e.name, e.line_number AS line",
		"MATCH (f)-[:CALLS*1..3]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT e.uid",
		"MATCH (caller)-[:CALLS]->(t) WHERE t.uid IN ['fn_f'] RETURN count(DISTINCT caller.uid)",
		"MATCH (f:File)-[:CONTAINS]->(e:Function) WHERE f.path = 'f.go' RETURN DISTINCT e.name, e.line_number",
	} {
		_, refusal, ok := parseCanonicalTraversal(query)
		if !ok {
			reason := "no reason given"
			if refusal != nil {
				reason = refusal.Error()
			}
			t.Errorf("refused a supported form: %s\n%s", query, reason)
		}
	}
}

// End to end through the backend, on a mounted canonical bundle: the refusal has to reach
// the caller as the error, not be swallowed or rewritten by the query wrapper.
func TestMountedCanonicalRefusalReachesTheCaller(t *testing.T) {
	mounted := buildCanonicalFixture(t)

	_, err := mounted.Query(context.Background(),
		"MATCH (f)-[:CALLS]->(e) WHERE f.uid IN ['fn_a'] RETURN DISTINCT label(e) AS type, e.name", nil)
	if err == nil {
		t.Fatal("want a refusal for a projected label")
	}
	if !strings.Contains(err.Error(), "label is not projectable") {
		t.Fatalf("refusal did not survive the call: %v", err)
	}
	if strings.Contains(err.Error(), "ladybug query:") {
		t.Errorf("refusal buried behind a wrapper prefix: %v", err)
	}

	res, err := mounted.Query(context.Background(),
		"MATCH (f)-[:CALLS]->(e:Function) WHERE f.uid IN ['fn_a'] RETURN DISTINCT e.name", nil)
	if err != nil {
		t.Fatalf("the suggested fix was refused too: %v", err)
	}
	if len(res.Records) == 0 {
		t.Error("the suggested fix planned but answered nothing")
	}
}

func TestMountedCanonicalDoesNotRefusePhysicalMemberTables(t *testing.T) {
	mounted := buildCanonicalFixture(t)

	res, err := mounted.Query(context.Background(),
		"MATCH ()-[r:calls__function_function]->() RETURN count(r) AS c", nil)
	if err != nil {
		t.Fatalf("a physical member table must reach the engine, not a refusal: %v", err)
	}
	if len(res.Records) == 0 {
		t.Error("counted nothing over a member table the fixture populates")
	}
}
