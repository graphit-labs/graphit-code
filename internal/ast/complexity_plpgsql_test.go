package ast

import (
	"testing"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	antlrPostgreSQL "github.com/graphit-labs/graphit-code/internal/ast/antlr/postgresql"
)

func findRuleNamed(n *antlrcommon.TreeNode, rule string) *antlrcommon.TreeNode {
	if n == nil {
		return nil
	}
	if n.Rule == rule {
		return n
	}
	for _, c := range n.Children {
		if r := findRuleNamed(c, rule); r != nil {
			return r
		}
	}
	return nil
}

// TestComplexityPlpgsqlSplicedIntoPostgresqlEntity is the end-to-end check:
// parse a real CREATE FUNCTION through the ANTLR PostgreSQL driver (which
// splices the LANGUAGE plpgsql body in via the real PL/pgSQL grammar — see
// plpgsql_splice.go), find the createfunctionstmt node the "functions" query
// in postgresql.yaml resolves to as the entity's scope, and confirm
// antlrComplexityMatcher.score walks into the spliced subtree and counts its
// branches — the same way it would for a native ANTLR dialect. This is what
// proves the two pieces (the splice, and postgresql.yaml's complexity:
// block naming PL/pgSQL's own rules) actually work together, not just each
// in isolation.
func TestComplexityPlpgsqlSplicedIntoPostgresqlEntity(t *testing.T) {
	src := `
CREATE FUNCTION f(x INTEGER) RETURNS INTEGER AS $$
BEGIN
  IF x > 0 THEN
    RETURN x;
  ELSIF x < 0 THEN
    RETURN -x;
  ELSE
    RETURN 0;
  END IF;
  FOR i IN 1..10 LOOP
    CONTINUE;
  END LOOP;
  CASE x
    WHEN 1 THEN RETURN 1;
    ELSE RETURN 0;
  END CASE;
END;
$$ LANGUAGE plpgsql;
`
	d := &antlrPostgreSQL.Driver{}
	tree, err := d.Parse([]byte(src))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	fn := findRuleNamed(tree, "createfunctionstmt")
	if fn == nil {
		t.Fatal("createfunctionstmt not found")
	}

	langConfig := &ExternalQueryFile{
		Complexity: &ComplexityConfig{
			NodeTypes: []string{"stmt_if", "elsif_clause", "stmt_for", "stmt_while", "stmt_foreach_a", "case_when", "proc_exception"},
		},
	}
	m := newAntlrComplexityMatcher(langConfig)
	if !m.on {
		t.Fatal("matcher did not activate")
	}
	if got, want := m.score(fn), 5; got != want {
		t.Errorf("score() = %d, want %d", got, want)
	}
}
