package main

import (
	"encoding/binary"
	"strings"
	"unsafe"

	"github.com/antlr4-go/antlr/v4"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/plsql/parser"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared"
)

func main() {}

var allocations = make(map[uintptr][]byte)

//go:wasmexport malloc
func malloc(size int32) uint32 {
	buf := make([]byte, size)
	ptr := uintptr(unsafe.Pointer(&buf[0]))
	allocations[ptr] = buf
	return uint32(ptr)
}

//go:wasmexport free
func free(ptr uint32) {
	delete(allocations, uintptr(ptr))
}

//go:wasmexport parse_antlr
func parse_antlr(sourcePtr uint32, sourceLen int32) uint32 {
	if sourceLen <= 0 {
		return 0
	}

	sourceBytes := unsafe.Slice((*byte)(unsafe.Pointer(uintptr(sourcePtr))), sourceLen)
	source := string(sourceBytes)

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

	var jsonStr string
	if tree == nil {
		jsonStr = `{"type":"error","message":"parse_error"}`
	} else {
		var out strings.Builder
		out.Grow(len(source) * 2)
		shared.TreeToJSON(&out, tree, shared.ParserMeta{
			RuleNames:     p.RuleNames,
			SymbolicNames: p.SymbolicNames,
			LiteralNames:  p.LiteralNames,
		})
		jsonStr = out.String()
	}

	respLen := len(jsonStr)
	buf := make([]byte, 4+respLen)
	binary.BigEndian.PutUint32(buf[0:4], uint32(respLen))
	copy(buf[4:], jsonStr)

	ptr := uintptr(unsafe.Pointer(&buf[0]))
	allocations[ptr] = buf
	return uint32(ptr)
}
