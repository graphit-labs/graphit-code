package db2

import (
	"testing"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

func TestCreateProcedureBodyParsesControlFlow(t *testing.T) {
	src := `
CREATE PROCEDURE f (IN x INT)
LANGUAGE SQL
BEGIN
  DECLARE y INT;
  IF x > 0 THEN
    SET y = x;
  ELSEIF x < 0 THEN
    SET y = -x;
  ELSE
    SET y = 0;
  END IF;
  WHILE x > 0 DO
    SET x = x - 1;
  END WHILE;
  CASE
    WHEN x = 1 THEN SET y = 1;
    ELSE SET y = 0;
  END CASE;
END
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

	for _, want := range []string{
		"compound_sql_inlined", "sql_procedure_body", "declare_variable_statement",
		"if_statement", "while_statement", "case_statement",
	} {
		if found[want] == 0 {
			t.Errorf("expected to find rule %q, found none — the procedure body was not attached to the CREATE PROCEDURE statement", want)
		}
	}
	if got, want := found["assignment_statement"], 6; got != want {
		t.Errorf("assignment_statement count = %d, want %d (one per SET)", got, want)
	}
}

// TestExistingSchemaExtractionStillWorks guards against the grammar edit
// regressing ordinary schema statements, which have nothing to do with
// procedure bodies.
func TestExistingSchemaExtractionStillWorks(t *testing.T) {
	src := `
CREATE TABLE employees (
  id INT NOT NULL,
  name VARCHAR(100),
  PRIMARY KEY (id)
);
CREATE VIEW active_employees AS SELECT id, name FROM employees WHERE id > 0;
`
	d := &Driver{}
	tree, err := d.Parse([]byte(src))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	found := map[string]bool{}
	var walk func(n *antlrcommon.TreeNode)
	walk = func(n *antlrcommon.TreeNode) {
		if n == nil {
			return
		}
		if n.IsRule() {
			found[n.Rule] = true
		}
		for _, c := range n.Children {
			walk(c)
		}
	}
	walk(tree)
	for _, want := range []string{"create_table_statement", "create_view_statement"} {
		if !found[want] {
			t.Errorf("regression: %s no longer found", want)
		}
	}
}
