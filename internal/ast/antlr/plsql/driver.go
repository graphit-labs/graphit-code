package plsql

import (
	"fmt"

	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// Driver implements antlrcommon.GrammarDriver for Oracle PL/SQL.
type Driver struct{}

func (d *Driver) Parse(src []byte) (*antlrcommon.TreeNode, error) {
	antlrcommon.LockParse()
	defer antlrcommon.UnlockParse()

	preprocessed := Preprocess(string(src))
	input := antlr.NewInputStream(preprocessed)
	lexer := NewPlSqlLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
	p := NewPlSqlParser(tokens)
	p.RemoveErrorListeners()
	nativeTree := antlrcommon.Parse(antlrcommon.LLOnly, p,
		func() antlr.ParseTree { return p.Sql_script() },
		func(mode int) { antlrcommon.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
	)
	if nativeTree == nil {
		return nil, fmt.Errorf("native antlr parse plsql failed")
	}
	converted := antlrcommon.ConvertParseTree(nativeTree, p.RuleNames, p.SymbolicNames, p.LiteralNames)
	// Comments live on the hidden channel and never reach the tree.
	if converted != nil {
		converted.Comments = antlrcommon.CollectComments(tokens, p.SymbolicNames)
	}
	return converted, nil
}
