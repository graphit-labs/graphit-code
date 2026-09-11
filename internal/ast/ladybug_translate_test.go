package ast

import (
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"
)

func translated(t *testing.T, q string) string {
	t.Helper()
	out, _ := translateLadybug(q, nil)
	return out
}

// The reported failure, reduced. `WHERE n:Function` is Neo4j syntax that Ladybug's
// parser rejects; the rewrite to label(n) = '...' exists for exactly this, but the
// escaping pass ran first and backticked the label, so the rewrite could no longer
// match. It therefore fired only for labels MISSING from the escape list — which is
// why `WHERE n:Method` worked (Method was missing) and `WHERE n:Function` crashed
// with a Parser exception. Inverted: the better-known the label, the surer it broke.
func TestTranslateRewritesLabelPredicateForKnownLabels(t *testing.T) {
	t.Parallel()

	for _, label := range []string{"Function", "Method", "Struct", "Interface", "File", "Table", "Heading"} {
		got := translated(t, "MATCH (n) WHERE n:"+label+" RETURN count(n)")
		want := "label(n) = '" + label + "'"
		if !strings.Contains(got, want) {
			t.Errorf("WHERE n:%s did not become %s:\n  %s", label, want, got)
		}
		if strings.Contains(got, "n:`"+label+"`") {
			t.Errorf("WHERE n:%s was escaped instead of rewritten — the old inversion is back:\n  %s", label, got)
		}
	}
}

// The query as the agent wrote it: a parenthesised OR group. The old regex demanded
// WHERE/AND/OR glued to the variable, so `AND (fn:Function` — with the paren — never
// matched, and the first alternative of any group was left in the broken form even
// when the rest were rewritten.
func TestTranslateRewritesEveryAlternativeInAParenthesisedGroup(t *testing.T) {
	t.Parallel()

	got := translated(t, "MATCH (f:File)-[:CONTAINS]->(fn) WHERE f.relative_path STARTS WITH 'internal/wikisvc/' "+
		"AND (fn:Function OR fn:Struct OR fn:Method OR fn:Interface) RETURN fn.name")

	for _, label := range []string{"Function", "Struct", "Method", "Interface"} {
		if !strings.Contains(got, "label(fn) = '"+label+"'") {
			t.Errorf("alternative %s not rewritten:\n  %s", label, got)
		}
	}
	if strings.Contains(got, "fn:") {
		t.Errorf("a label predicate survived, which is the shape the parser rejects:\n  %s", got)
	}
	for _, want := range []string{"(f:`File`)", "[:`CONTAINS`]"} {
		if !strings.Contains(got, want) {
			t.Errorf("pattern label lost or unescaped, missing %s:\n  %s", want, got)
		}
	}
}

func TestTranslateHandlesNotAndNestedParens(t *testing.T) {
	t.Parallel()

	got := translated(t, "MATCH (n) WHERE NOT n:Function AND ((n:Struct)) RETURN n.name")
	if !strings.Contains(got, "NOT label(n) = 'Function'") {
		t.Errorf("NOT prefix not preserved:\n  %s", got)
	}
	if !strings.Contains(got, "((label(n) = 'Struct'))") {
		t.Errorf("nested parens not handled:\n  %s", got)
	}
}

// Escaping by position instead of by name is what removes the drift: a label is a
// label because of where it sits, so a grammar shipped tomorrow needs no code change.
func TestTranslateEscapesLabelsByPositionNotByName(t *testing.T) {
	t.Parallel()

	cases := map[string]string{
		"MATCH (n:NeverHeardOfIt) RETURN n.name":     "(n:`NeverHeardOfIt`)",
		"MATCH (:Anonymous) RETURN 1":                "(:`Anonymous`)",
		"MATCH ()-[:BRAND_NEW_REL]->() RETURN 1":     "[:`BRAND_NEW_REL`]",
		"MATCH ()-[r:AlsoNew]->() RETURN r":          "[r:`AlsoNew`]",
		"MATCH (n : Spaced) RETURN n":                "(n : `Spaced`)",
		"MATCH ()-[:CALLS*1..3]->(x:Deep) RETURN x":  "(x:`Deep`)",
		"MATCH (n:Function) RETURN n.name":           "(n:`Function`)",
		"MATCH (a)-[r:CALLS|IMPORTS]->(b) RETURN a":  "[r:`CALLS`|`IMPORTS`]",
		"MATCH (n:`Function`) RETURN n.name":         "(n:`Function`)",
		"MATCH (f:File {path: '__config__'}) RETURN": "(f:`File` {path: '__config__'})",
	}

	for q, want := range cases {
		if got := translated(t, q); !strings.Contains(got, want) {
			t.Errorf("%q\n  want substring %q\n  got  %s", q, want, got)
		}
	}
}

func TestTranslateEscapesEveryRelationshipTypeAlternative(t *testing.T) {
	t.Parallel()

	cases := map[string]string{
		"MATCH (a)-[r:CALLS|IMPORTS]->(b) RETURN a":          "[r:`CALLS`|`IMPORTS`]",
		"MATCH (a)-[:CALLS|IMPORTS|INHERITS]->(b) RETURN a":  "[:`CALLS`|`IMPORTS`|`INHERITS`]",
		"MATCH (a)-[r:CALLS | IMPORTS]->(b) RETURN a":        "[r:`CALLS` | `IMPORTS`]",
		"MATCH (a)-[r:CALLS|:IMPORTS]->(b) RETURN a":         "[r:`CALLS`|:`IMPORTS`]",
		"MATCH (a)-[r:CALLS*1..3]->(b) RETURN a":             "[r:`CALLS`*1..3]",
		"MATCH (a)-[r:CALLS {line_number: 3}]->(b) RETURN a": "[r:`CALLS` {line_number: 3}]",
	}

	for q, want := range cases {
		if got := translated(t, q); !strings.Contains(got, want) {
			t.Errorf("%q\n  want substring %q\n  got  %s", q, want, got)
		}
	}

	for _, q := range []string{
		"MATCH (n:Function) RETURN [x IN [1, 2] | x] AS xs",
		"RETURN [a IN labels | a] AS ls",
	} {
		if got := translated(t, q); strings.Contains(got, "|`") {
			t.Errorf("a list comprehension was treated as a type alternation:\n  in:  %s\n  out: %s", q, got)
		}
	}
}

// Every label the shipped grammars can produce must survive both positions. This is
// the guard the hardcoded list never had: it was 64 of 114 labels behind, and nothing
// failed when a grammar added one.
func TestTranslateCoversEveryShippedGrammarLabel(t *testing.T) {
	t.Parallel()

	labels := shippedGraphLabels(t)
	if len(labels) < 50 {
		t.Fatalf("expected the grammar files to declare many labels, found %d", len(labels))
	}

	for _, label := range labels {
		pattern := translated(t, "MATCH (n:"+label+") RETURN n.name")
		if !strings.Contains(pattern, "(n:`"+label+"`)") {
			t.Errorf("label %q not escaped in a pattern position:\n  %s", label, pattern)
		}

		predicate := translated(t, "MATCH (n) WHERE n:"+label+" RETURN n.name")
		if !strings.Contains(predicate, "label(n) = '"+label+"'") {
			t.Errorf("label %q not rewritten in a WHERE predicate:\n  %s", label, predicate)
		}
	}
}

// A rewrite reaching into a string literal changes what the query asks for, and says
// nothing while doing it. The uid case is not hypothetical: a uid reads
// `internal/x/y.go::Apply.Apply`, and `::Apply` is exactly the shape being matched.
func TestTranslateLeavesStringLiteralsAlone(t *testing.T) {
	t.Parallel()

	cases := []struct{ query, literal string }{
		{"MATCH (n:Function) WHERE n.uid = 'internal/x/y.go::Apply.Apply' RETURN n.path", "'internal/x/y.go::Apply.Apply'"},
		{"MATCH (c:Comment) WHERE c.name CONTAINS '(x:Function)' RETURN c.path", "'(x:Function)'"},
		{"MATCH (c:Comment) WHERE c.name CONTAINS 'labels(n)[0]' RETURN c.path", "'labels(n)[0]'"},
		{`MATCH (c:Comment) WHERE c.name CONTAINS "type(" RETURN c.path`, `"type("`},
		{"MATCH (c:Comment) WHERE c.name CONTAINS 'WHERE x:Thing' RETURN c.path", "'WHERE x:Thing'"},
		{`MATCH (c:Comment) WHERE c.name = 'it\'s (n:Escaped)' RETURN c.path`, `'it\'s (n:Escaped)'`},
	}

	for _, c := range cases {
		if got := translated(t, c.query); !strings.Contains(got, c.literal) {
			t.Errorf("string literal was rewritten:\n  in:  %s\n  out: %s", c.query, got)
		}
	}
}

func TestTranslateNoopsOnlyRealDDLStatements(t *testing.T) {
	t.Parallel()

	for _, q := range []string{
		"CREATE INDEX Function_name IF NOT EXISTS FOR (n:Function) ON (n.name)",
		"CREATE CONSTRAINT ON (n:Function) ASSERT n.uid IS UNIQUE",
	} {
		if got := translated(t, q); got != "RETURN 1" {
			t.Errorf("real DDL should be a no-op, got: %s", got)
		}
	}

	q := "MATCH (c:Comment) WHERE c.name CONTAINS 'CREATE INDEX' RETURN c.path"
	got := translated(t, q)
	if got == "RETURN 1" {
		t.Error("a query searching for the text 'CREATE INDEX' was swallowed as DDL")
	}
	if !strings.Contains(got, "'CREATE INDEX'") {
		t.Errorf("the search term was mangled:\n  %s", got)
	}
}

func TestTranslateRewritesLabelsIndexForAnyVariable(t *testing.T) {
	t.Parallel()

	got := translated(t, "MATCH (f:Function) RETURN labels(f)[0] AS kind, labels(n)[0] AS other")
	for _, want := range []string{"label(f) AS kind", "label(n) AS other"} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q:\n  %s", want, got)
		}
	}
}

// The DDL the indexer itself emits must pass through untouched — a stray backtick or
// a mangled column list aborts a rebuild, and after --reset that leaves no database.
func TestTranslateLeavesNodeTableDDLIntact(t *testing.T) {
	t.Parallel()

	for _, ddl := range []string{
		"CREATE NODE TABLE IF NOT EXISTS `Function`(uid STRING, name STRING, path STRING, PRIMARY KEY (uid))",
		"CREATE REL TABLE IF NOT EXISTS IMPORTS(FROM File TO Module, alias STRING, line_number INT64)",
		"CREATE NODE TABLE IF NOT EXISTS File(path STRING, name STRING, source STRING, PRIMARY KEY (path))",
	} {
		if got := translated(t, ddl); got != ddl {
			t.Errorf("DDL was rewritten:\n  in:  %s\n  out: %s", ddl, got)
		}
	}
}

func shippedGraphLabels(t *testing.T) []string {
	t.Helper()

	entries, err := os.ReadDir("queries")
	if err != nil {
		t.Fatalf("read grammar dir: %v", err)
	}

	re := regexp.MustCompile(`(?m)^\s*graph_label:\s*"?([A-Za-z_][A-Za-z0-9_]*)"?`)
	seen := map[string]bool{}

	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".yaml" {
			continue
		}
		data, err := os.ReadFile(filepath.Join("queries", e.Name()))
		if err != nil {
			t.Fatalf("read %s: %v", e.Name(), err)
		}
		for _, m := range re.FindAllStringSubmatch(string(data), -1) {
			seen[m[1]] = true
		}
	}

	out := make([]string, 0, len(seen))
	for l := range seen {
		out = append(out, l)
	}
	sort.Strings(out)
	return out
}
