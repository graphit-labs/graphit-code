// ANTLR4 PL/SQL parser driver — native binary or WASM (GOOS=wasip1).
//
// Uses the shared driver for IPC, stdout protection, and SLL→LL parsing.
// The only grammar-specific code is the preprocessor and the start rule.
package main

import (
	"github.com/antlr4-go/antlr/v4"

	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/plsql/parser"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared"
)

func main() {
	shared.Run(func(source string) shared.ParseResult {
		preprocessed := Preprocess(source)

		input := antlr.NewInputStream(preprocessed)
		lexer := parser.NewPlSqlLexer(input)
		lexer.RemoveErrorListeners()
		tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)

		p := parser.NewPlSqlParser(tokens)
		p.RemoveErrorListeners()

		tree := shared.ParseSLLThenLL(
			lexer,
			func() antlr.ParseTree { return p.Sql_script() },
			func(mode int) { shared.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
		)

		return shared.ParseResult{
			Tree: tree,
			Meta: shared.ParserMeta{
				RuleNames:     p.RuleNames,
				SymbolicNames: p.SymbolicNames,
				LiteralNames:  p.LiteralNames,
			},
		}
	})
}
