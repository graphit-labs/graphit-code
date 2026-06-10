package plsql

import "github.com/antlr4-go/antlr/v4"

// PlSqlLexerBase provides the base class required by PlSqlLexer.g4.
// Port from Java: grammars-v4/sql/plsql/Java/PlSqlLexerBase.java
type PlSqlLexerBase struct {
	*antlr.BaseLexer
}

// IsNewlineAtPos checks if char at lookahead position is newline or EOF.
func (l *PlSqlLexerBase) IsNewlineAtPos(pos int) bool {
	la := l.GetInputStream().LA(pos)
	return la == -1 || la == '\n'
}
