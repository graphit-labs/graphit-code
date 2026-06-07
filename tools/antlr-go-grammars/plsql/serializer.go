package main

import (
	"fmt"
	"strings"

	"github.com/antlr4-go/antlr/v4"
)

// treeToJSON serializes an ANTLR parse tree to JSON matching the format
// expected by the host (wasmantlr/tree.go TreeNode):
//
//	Rule node:  {"rule":"sql_script","start":[1,0],"end":[10,20],"children":[...]}
//	Token node: {"token":"CREATE","text":"CREATE","start":[1,0],"end":[1,5]}
//
// EOF tokens are excluded.
func treeToJSON(out *strings.Builder, node antlr.Tree, ruleNames, symbolicNames, literalNames []string) {
	switch n := node.(type) {
	case antlr.TerminalNode:
		tok := n.GetSymbol()
		if tok.GetTokenType() == antlr.TokenEOF {
			return
		}

		out.WriteString(`{"token":"`)
		name := tokenDisplayName(tok.GetTokenType(), symbolicNames, literalNames)
		escapeJSON(out, name)
		out.WriteString(`","text":"`)
		escapeJSON(out, tok.GetText())
		out.WriteString(`","start":[`)
		fmt.Fprintf(out, "%d,%d", tok.GetLine(), tok.GetColumn())
		out.WriteString(`],"end":[`)
		text := tok.GetText()
		endCol := tok.GetColumn() + len(text) - 1
		fmt.Fprintf(out, "%d,%d", tok.GetLine(), endCol)
		out.WriteString(`]}`)

	case antlr.ParserRuleContext:
		out.WriteString(`{"rule":"`)
		ruleIdx := n.GetRuleIndex()
		if ruleIdx >= 0 && ruleIdx < len(ruleNames) {
			escapeJSON(out, ruleNames[ruleIdx])
		}
		out.WriteByte('"')

		start := n.GetStart()
		stop := n.GetStop()
		if start != nil {
			fmt.Fprintf(out, `,"start":[%d,%d]`, start.GetLine(), start.GetColumn())
		}
		if stop != nil {
			endCol := stop.GetColumn() + len(stop.GetText()) - 1
			fmt.Fprintf(out, `,"end":[%d,%d]`, stop.GetLine(), endCol)
		}

		children := n.GetChildren()
		if len(children) > 0 {
			out.WriteString(`,"children":[`)
			first := true
			for _, child := range children {
				if t, ok := child.(antlr.TerminalNode); ok {
					if t.GetSymbol().GetTokenType() == antlr.TokenEOF {
						continue
					}
				}

				var tmp strings.Builder
				treeToJSON(&tmp, child, ruleNames, symbolicNames, literalNames)
				if tmp.Len() == 0 {
					continue
				}

				if !first {
					out.WriteByte(',')
				}
				out.WriteString(tmp.String())
				first = false
			}
			out.WriteByte(']')
		}
		out.WriteByte('}')
	}
}

// tokenDisplayName replicates ANTLR4's Vocabulary.getDisplayName:
// literal name if available, then symbolic name, then numeric.
func tokenDisplayName(tokenType int, symbolicNames, literalNames []string) string {
	if tokenType >= 0 && tokenType < len(literalNames) && literalNames[tokenType] != "" {
		return literalNames[tokenType]
	}
	if tokenType >= 0 && tokenType < len(symbolicNames) && symbolicNames[tokenType] != "" {
		return symbolicNames[tokenType]
	}
	return fmt.Sprintf("%d", tokenType)
}

func escapeJSON(out *strings.Builder, s string) {
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch c {
		case '"':
			out.WriteString(`\"`)
		case '\\':
			out.WriteString(`\\`)
		case '\n':
			out.WriteString(`\n`)
		case '\r':
			out.WriteString(`\r`)
		case '\t':
			out.WriteString(`\t`)
		default:
			if c < 0x20 {
				fmt.Fprintf(out, `\u%04x`, c)
			} else {
				out.WriteByte(c)
			}
		}
	}
}
