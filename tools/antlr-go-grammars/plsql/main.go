// ANTLR4 PL/SQL WASM parser driver — compiled with GOOS=wasip1 GOARCH=wasm.
//
// Protocol: length-prefixed IPC over stdin/stdout.
//   Request:  [4 bytes BE uint32 length][source bytes]
//   Response: [4 bytes BE uint32 length][JSON parse tree]
//
// Runs as a persistent loop reading requests until stdin closes.
// ATN tables are initialized once on the first parse and reused.
//
// Parsing uses two-stage SLL→LL with panic recovery:
//   Stage 1: SLL + BailErrorStrategy (fast path, O(n))
//   Stage 2: LL + DefaultErrorStrategy (full recovery, on SLL failure)
package main

import (
	"encoding/binary"
	"io"
	"os"
	"strings"

	"github.com/antlr4-go/antlr/v4"

	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/plsql/parser"
)

func main() {
	stdin := os.Stdin
	stdout := os.Stdout

	var lenBuf [4]byte

	for {
		if _, err := io.ReadFull(stdin, lenBuf[:]); err != nil {
			break // stdin closed — clean exit
		}

		srcLen := binary.BigEndian.Uint32(lenBuf[:])
		source := make([]byte, srcLen)
		if _, err := io.ReadFull(stdin, source); err != nil {
			os.Exit(1)
		}

		preprocessed := Preprocess(string(source))

		json := parseAndSerialize(preprocessed)

		binary.BigEndian.PutUint32(lenBuf[:], uint32(len(json)))
		stdout.Write(lenBuf[:])
		stdout.Write([]byte(json))
	}
}

func parseAndSerialize(source string) string {
	input := antlr.NewInputStream(source)
	lexer := parser.NewPlSqlLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)

	p := parser.NewPlSqlParser(tokens)
	p.RemoveErrorListeners()
	p.BuildParseTrees = true

	// SLL fast path — falls back to LL on ambiguity.
	tree := trySLLParse(p, tokens)

	if tree == nil {
		tree = llParse(p, tokens)
	}

	if tree == nil {
		return `{"type":"error","message":"parse_error"}`
	}

	var out strings.Builder
	out.Grow(len(source) * 2)
	treeToJSON(&out, tree, p.RuleNames, p.SymbolicNames, p.LiteralNames)
	return out.String()
}

// trySLLParse attempts SLL prediction. Returns nil on failure (BailErrorStrategy panics).
func trySLLParse(p *parser.PlSqlParser, tokens *antlr.CommonTokenStream) (tree antlr.ParseTree) {
	defer func() {
		if r := recover(); r != nil {
			tree = nil
		}
	}()

	p.Interpreter.SetPredictionMode(antlr.PredictionModeSLL)
	p.SetErrorHandler(antlr.NewBailErrorStrategy())
	tree = p.Sql_script()
	return tree
}

// llParse runs LL prediction with DefaultErrorStrategy (full error recovery).
func llParse(p *parser.PlSqlParser, tokens *antlr.CommonTokenStream) antlr.ParseTree {
	defer func() {
		if r := recover(); r != nil {
		}
	}()

	tokens.Seek(0)
	p.RemoveErrorListeners()
	p.Interpreter.SetPredictionMode(antlr.PredictionModeLL)
	p.SetErrorHandler(antlr.NewDefaultErrorStrategy())
	p.BuildParseTrees = true
	return p.Sql_script()
}
