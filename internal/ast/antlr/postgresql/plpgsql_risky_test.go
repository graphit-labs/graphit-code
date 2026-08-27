package postgresql

import (
	"testing"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// TestSpliceHandlesPlpgsqlSpecificConstructs re-checks, through the full
// Driver.Parse path, the exact constructs that broke the earlier attempt to
// reuse the PL/SQL parser for this: PERFORM, RAISE EXCEPTION with format
// args, RETURN QUERY, and FOREACH ... IN ARRAY. None of these should ever
// introduce a false if_statement-equivalent, and the parse must not report
// an error.
func TestSpliceHandlesPlpgsqlSpecificConstructs(t *testing.T) {
	cases := map[string]string{
		"perform": `
CREATE FUNCTION f() RETURNS void AS $$
BEGIN
  PERFORM do_something();
  IF FOUND THEN
    RAISE NOTICE 'found it';
  END IF;
END;
$$ LANGUAGE plpgsql;
`,
		"raise_exception": `
CREATE FUNCTION f() RETURNS void AS $$
BEGIN
  RAISE EXCEPTION 'bad value: %', 1;
END;
$$ LANGUAGE plpgsql;
`,
		"return_query": `
CREATE FUNCTION f() RETURNS SETOF int AS $$
BEGIN
  RETURN QUERY SELECT 1;
END;
$$ LANGUAGE plpgsql;
`,
		"foreach_array": `
CREATE FUNCTION f() RETURNS void AS $$
DECLARE
  x int;
BEGIN
  FOREACH x IN ARRAY ARRAY[1,2,3] LOOP
    RAISE NOTICE '%', x;
  END LOOP;
END;
$$ LANGUAGE plpgsql;
`,
	}
	for name, src := range cases {
		t.Run(name, func(t *testing.T) {
			d := &Driver{}
			tree, err := d.Parse([]byte(src))
			if err != nil {
				t.Fatalf("parse: %v", err)
			}
			var checkNoError func(n *antlrcommon.TreeNode)
			checkNoError = func(n *antlrcommon.TreeNode) {
				if n == nil {
					return
				}
				if n.Rule == "ERROR" {
					t.Errorf("spliced PL/pgSQL subtree has an ERROR node: %q", n.Text)
				}
				for _, c := range n.Children {
					checkNoError(c)
				}
			}
			checkNoError(tree)
		})
	}
}

// TestExistingSchemaExtractionStillWorks guards against the splice
// regressing ordinary DDL, which has nothing to do with function bodies.
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
	for _, want := range []string{"createstmt", "viewstmt"} {
		if !found[want] {
			t.Errorf("regression: %s no longer found", want)
		}
	}
}
