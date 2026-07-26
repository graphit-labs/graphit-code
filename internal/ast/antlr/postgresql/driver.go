package postgresql

import (
	"fmt"

	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// Driver implements antlrcommon.GrammarDriver for PostgreSQL.
type Driver struct{}

func (d *Driver) Parse(src []byte) (*antlrcommon.TreeNode, error) {
	antlrcommon.LockParse()
	defer antlrcommon.UnlockParse()

	input := antlr.NewInputStream(string(src))
	lexer := NewPostgreSQLLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
	p := NewPostgreSQLParser(tokens)
	p.RemoveErrorListeners()
	nativeTree := antlrcommon.Parse(antlrcommon.LLOnly, p,
		func() antlr.ParseTree { return p.Root() },
		func(mode int) { antlrcommon.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
	)
	if nativeTree == nil {
		return nil, fmt.Errorf("antlr parse postgresql failed")
	}
	return antlrcommon.ConvertParseTree(nativeTree, p.RuleNames, p.SymbolicNames, p.LiteralNames), nil
}
