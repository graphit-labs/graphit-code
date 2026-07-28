package cobol85

import (
	"fmt"

	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// Driver implements antlrcommon.GrammarDriver for COBOL 85.
type Driver struct{}

func (d *Driver) Parse(src []byte) (*antlrcommon.TreeNode, error) {
	antlrcommon.LockParse()
	defer antlrcommon.UnlockParse()

	preprocessed := Preprocess(string(src))
	input := antlr.NewInputStream(preprocessed)
	lexer := NewCobol85Lexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
	p := NewCobol85Parser(tokens)
	p.RemoveErrorListeners()
	nativeTree := antlrcommon.Parse(antlrcommon.LLOnly, p,
		func() antlr.ParseTree { return p.StartRule() },
		func(mode int) { antlrcommon.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
	)
	if nativeTree == nil {
		return nil, fmt.Errorf("antlr parse cobol85 failed")
	}
	converted := antlrcommon.ConvertParseTree(nativeTree, p.RuleNames, p.SymbolicNames, p.LiteralNames)
	// Comments live on the hidden channel and never reach the tree.
	if converted != nil {
		converted.Comments = antlrcommon.CollectComments(tokens, p.SymbolicNames)
	}
	return converted, nil
}
