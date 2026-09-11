package antlrcommon

// GrammarDriver abstracts grammar-specific ANTLR parsing.
// Each grammar (PL/SQL, PostgreSQL, T-SQL, DB2, etc.) implements this
// interface so the adapter can dispatch parsing without if/else chains.
type GrammarDriver interface {
	Parse(src []byte) (*TreeNode, error)
}
