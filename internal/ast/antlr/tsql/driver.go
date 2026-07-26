package tsql

import (
	"fmt"

	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// Driver implements antlrcommon.GrammarDriver for T-SQL (SQL Server / Azure SQL).
type Driver struct{}

func (d *Driver) Parse(src []byte) (*antlrcommon.TreeNode, error) {
	antlrcommon.LockParse()
	defer antlrcommon.UnlockParse()

	input := antlr.NewInputStream(string(src))
	lexer := NewTSqlLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
	p := NewTSqlParser(tokens)
	p.RemoveErrorListeners()
	nativeTree := antlrcommon.Parse(antlrcommon.LLOnly,
		func() antlr.ParseTree { return p.Tsql_file() },
		func(mode int) { antlrcommon.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
	)
	if nativeTree == nil {
		return nil, fmt.Errorf("antlr parse tsql failed")
	}
	return antlrcommon.ConvertParseTree(nativeTree, p.RuleNames, p.SymbolicNames, p.LiteralNames), nil
}
