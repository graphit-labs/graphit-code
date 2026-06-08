// Package shared provides the reusable IPC driver, error strategy, and
// serializer for all ANTLR4 Go grammar binaries.
//
// Every grammar binary uses the same length-prefixed IPC protocol over
// stdin/stdout. This package eliminates duplication and, critically,
// protects the binary IPC wire from ANTLR's internal fmt.Println calls
// that would otherwise corrupt the stream.
//
// # Usage
//
// A grammar's main.go only needs ~30 lines:
//
//	package main
//
//	import (
//	    "github.com/antlr4-go/antlr/v4"
//	    "github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/mygrammar/parser"
//	    "github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared"
//	)
//
//	func main() {
//	    shared.Run(func(source string) shared.ParseResult {
//	        input := antlr.NewInputStream(source)
//	        lexer := parser.NewMyLexer(input)
//	        lexer.RemoveErrorListeners()
//	        tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
//
//	        p := parser.NewMyParser(tokens)
//	        p.RemoveErrorListeners()
//
//	        tree := shared.ParseSLLThenLL(
//	            lexer,
//	            func() antlr.ParseTree { return p.Start_rule() },
//	            func(mode int) { shared.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
//	        )
//
//	        return shared.ParseResult{
//	            Tree: tree,
//	            Meta: shared.ParserMeta{
//	                RuleNames:     p.RuleNames,
//	                SymbolicNames: p.SymbolicNames,
//	                LiteralNames:  p.LiteralNames,
//	            },
//	        }
//	    })
//	}
//
// # What the shared package handles
//
//   - IPC loop: length-prefixed binary protocol (4-byte BE uint32 + payload)
//   - Stdout protection: redirects os.Stdout to /dev/null so ANTLR's
//     internal fmt.Println calls don't corrupt the wire
//   - SLL→LL two-stage parsing with panic recovery
//   - SilentErrorStrategy: defense-in-depth against ANTLR error prints
//   - JSON serialization: converts parse trees to the host's expected format
//
// # go.mod setup
//
// Each grammar module adds:
//
//	require github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared v0.0.0
//	replace github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared => ../shared
package shared
