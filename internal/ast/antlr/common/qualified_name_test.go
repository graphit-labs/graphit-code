package antlrcommon

import "testing"

func rule(name string, children ...*TreeNode) *TreeNode {
	return &TreeNode{Rule: name, Children: children}
}

func term(text string) *TreeNode {
	return &TreeNode{Token: "T", Text: text}
}

func ident(text string) *TreeNode {
	return rule("identifier", rule("id_expression", term(text)))
}

// TestQualifiedNameTextTakesTheObjectNotTheQualifier covers the reading that made an
// Oracle export come out named after its schema: every sequence called GC, every DML
// edge pointing at one node called GC, every column attributed to a table of that name.
func TestQualifiedNameTextTakesTheObjectNotTheQualifier(t *testing.T) {
	tests := []struct {
		name string
		node *TreeNode
		want string
	}{
		{
			name: "schema qualified table",
			node: rule("tableview_name", ident(`"GC"`), term("."), rule("id_expression", term(`"PEDIDO"`))),
			want: `"PEDIDO"`,
		},
		{
			name: "bare table",
			node: rule("table_name", ident("PEDIDO")),
			want: "PEDIDO",
		},
		{
			name: "sequence",
			node: rule("sequence_name", rule("id_expression", term("GC")), term("."), rule("id_expression", term("SEQ_PEDIDO"))),
			want: "SEQ_PEDIDO",
		},
		{
			name: "package qualified call over a link",
			node: rule("routine_name", ident("PCK_VENDA"), term("."), rule("id_expression", term("CALCULA")),
				term("@"), rule("link_name", ident("REMOTO"))),
			want: "CALCULA",
		},
		{
			name: "three part column",
			node: rule("column_name", ident("GC"), term("."), rule("id_expression", term("PEDIDO")),
				term("."), rule("id_expression", term("ID_PEDIDO"))),
			want: "ID_PEDIDO",
		},
		{
			name: "name ends at a keyword",
			node: rule("synonym_clause", ident("SYN_PEDIDO"), term("FOR"), ident("PEDIDO")),
			want: "SYN_PEDIDO",
		},
		{
			name: "adjacent names do not merge",
			node: rule("create_trigger_head", rule("trigger_name", ident("TRG_PEDIDO")),
				rule("tableview_name", ident("PEDIDO"))),
			want: "TRG_PEDIDO",
		},
		{
			name: "quoted string is not a name node",
			node: rule("quoted_string", term("'Data de abertura. Preenchida pelo gatilho.'")),
			want: "",
		},
		{
			name: "keyword statement",
			node: rule("commit_statement", term("COMMIT"), term("WORK")),
			want: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.node.QualifiedNameText(); got != tt.want {
				t.Fatalf("QualifiedNameText() = %q, want %q", got, tt.want)
			}
		})
	}
}

// TestDeclaredNameTextFindsTheDeclaredObject covers context resolution, which read the
// first "_name" child and therefore read the SCHEMA of every CREATE statement.
func TestDeclaredNameTextFindsTheDeclaredObject(t *testing.T) {
	tests := []struct {
		name string
		node *TreeNode
		want string
	}{
		{
			name: "create table skips the schema and stops before the body",
			node: rule("create_table", term("CREATE"), term("TABLE"),
				rule("schema_name", ident(`"GC"`)), term("."),
				rule("table_name", ident(`"PEDIDO"`)),
				rule("relational_table", term("("), term(")"))),
			want: `"PEDIDO"`,
		},
		{
			name: "create view names with a bare id_expression",
			node: rule("create_view", term("CREATE"), term("OR"), term("REPLACE"), term("FORCE"),
				rule("editioning_clause", term("EDITIONING")), term("VIEW"),
				rule("schema_name", ident(`"GC"`)), term("."),
				rule("id_expression", term(`"VW_PEDIDO"`)),
				term("AS"), rule("select_only_statement", term("SELECT"))),
			want: `"VW_PEDIDO"`,
		},
		{
			name: "package level function body",
			node: rule("function_body", term("FUNCTION"), ident("CALCULA"), term("RETURN")),
			want: "CALCULA",
		},
		{
			name: "create type delegates the name to its definition",
			node: rule("create_type", term("CREATE"), term("TYPE"),
				rule("type_definition", rule("type_name", ident("TYP_PEDIDO")), term("AS"))),
			want: "TYP_PEDIDO",
		},
		{
			name: "create synonym stops at FOR",
			node: rule("create_synonym", term("CREATE"), term("SYNONYM"),
				rule("schema_name", ident("GC")), term("."),
				rule("synonym_name", ident("SYN_PEDIDO")),
				term("FOR"), rule("schema_name", ident("OUTRO")), term("."),
				rule("schema_object_name", ident("PEDIDO"))),
			want: "SYN_PEDIDO",
		},
		{
			name: "camel case declaration",
			node: rule("fileDescriptionEntry", term("FD"), rule("fileName", term("CLIENTES"))),
			want: "CLIENTES",
		},
		{
			name: "no name shaped child",
			node: rule("paragraph", rule("sentence", term("DISPLAY"))),
			want: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.node.DeclaredNameText(); got != tt.want {
				t.Fatalf("DeclaredNameText() = %q, want %q", got, tt.want)
			}
		})
	}
}
