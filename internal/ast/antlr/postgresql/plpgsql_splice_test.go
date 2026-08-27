package postgresql

import (
	"testing"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// TestSpliceFindsRealPlpgsqlBranches checks the full Driver.Parse path: a
// dollar-quoted LANGUAGE plpgsql body — otherwise one opaque string constant
// — gets its real control flow spliced in from the PL/pgSQL tree-sitter
// grammar, with the right rule names for cyclomatic complexity.
func TestSpliceFindsRealPlpgsqlBranches(t *testing.T) {
	src := `
CREATE OR REPLACE FUNCTION f(x INTEGER) RETURNS INTEGER AS $$
DECLARE
  y INTEGER;
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
  WHILE x > 0 LOOP
    EXIT;
  END LOOP;
  CASE x
    WHEN 1 THEN y := 1;
    ELSE y := 0;
  END CASE;
  RETURN y;
EXCEPTION
  WHEN division_by_zero THEN
    RETURN 0;
  WHEN OTHERS THEN
    RAISE;
END;
$$ LANGUAGE plpgsql;
`
	d := &Driver{}
	tree, err := d.Parse([]byte(src))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	found := map[string]int{}
	var walk func(n *antlrcommon.TreeNode)
	walk = func(n *antlrcommon.TreeNode) {
		if n == nil {
			return
		}
		if n.IsRule() {
			found[n.Rule]++
		}
		for _, c := range n.Children {
			walk(c)
		}
	}
	walk(tree)
	for _, want := range []string{"stmt_if", "elsif_clause", "stmt_for", "stmt_while", "case_when", "proc_exception"} {
		if found[want] == 0 {
			t.Errorf("expected to find PL/pgSQL rule %q spliced from the body, found none", want)
		}
	}
}

// TestSpliceIgnoresNonPlpgsqlLanguages confirms only LANGUAGE plpgsql is
// spliced — a LANGUAGE sql (or plpython3u, or anything else) body stays the
// opaque string it already was, not force-parsed by a grammar that does not
// describe it.
func TestSpliceIgnoresNonPlpgsqlLanguages(t *testing.T) {
	src := `CREATE FUNCTION f() RETURNS int AS $$ SELECT 1 $$ LANGUAGE sql;`
	d := &Driver{}
	tree, err := d.Parse([]byte(src))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	var walk func(n *antlrcommon.TreeNode)
	walk = func(n *antlrcommon.TreeNode) {
		if n == nil {
			return
		}
		if n.Rule == "pl_block" || n.Rule == "stmt_if" {
			t.Error("a LANGUAGE sql body must not be parsed by the PL/pgSQL grammar")
		}
		for _, c := range n.Children {
			walk(c)
		}
	}
	walk(tree)
}

// TestSpliceDoesNotTouchOrdinaryStringConstants confirms the splice only
// fires for the dollar-quoted alternative of anysconst.
func TestSpliceDoesNotTouchOrdinaryStringConstants(t *testing.T) {
	src := `CREATE FUNCTION f() RETURNS INTEGER AS 'IF x > 0 THEN RETURN 1; END IF;' LANGUAGE plpgsql;`
	d := &Driver{}
	tree, err := d.Parse([]byte(src))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	var walk func(n *antlrcommon.TreeNode)
	walk = func(n *antlrcommon.TreeNode) {
		if n == nil {
			return
		}
		if n.Rule == "stmt_if" {
			t.Error("stmt_if should not be spliced into an ordinary quoted string")
		}
		for _, c := range n.Children {
			walk(c)
		}
	}
	walk(tree)
}
