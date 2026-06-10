package antlrcommon

import (
	"fmt"

	"github.com/antlr4-go/antlr/v4"
)

// ConvertParseTree converts a native ANTLR parse tree into a serializable TreeNode tree.
// This is the shared conversion used by all grammar drivers.
func ConvertParseTree(node antlr.Tree, ruleNames, symbolicNames, literalNames []string) *TreeNode {
	switch n := node.(type) {
	case antlr.TerminalNode:
		tok := n.GetSymbol()
		if tok.GetTokenType() == antlr.TokenEOF {
			return nil
		}

		name := tokenDisplayName(tok.GetTokenType(), symbolicNames, literalNames)
		text := tok.GetText()
		endCol := tok.GetColumn() + len(text) - 1
		return &TreeNode{
			Token: name,
			Text:  text,
			Start: [2]int{tok.GetLine(), tok.GetColumn()},
			End:   [2]int{tok.GetLine(), endCol},
		}

	case antlr.ParserRuleContext:
		var ruleName string
		ruleIdx := n.GetRuleIndex()
		if ruleIdx >= 0 && ruleIdx < len(ruleNames) {
			ruleName = ruleNames[ruleIdx]
		}

		startLine, startCol := 0, 0
		start := n.GetStart()
		if start != nil {
			startLine, startCol = start.GetLine(), start.GetColumn()
		}

		endLine, endCol := 0, 0
		stop := n.GetStop()
		if stop != nil {
			endLine = stop.GetLine()
			endCol = stop.GetColumn() + len(stop.GetText()) - 1
		}

		var children []*TreeNode
		antlrChildren := n.GetChildren()
		for _, child := range antlrChildren {
			if t, ok := child.(antlr.TerminalNode); ok {
				if t.GetSymbol().GetTokenType() == antlr.TokenEOF {
					continue
				}
			}
			converted := ConvertParseTree(child, ruleNames, symbolicNames, literalNames)
			if converted != nil {
				children = append(children, converted)
			}
		}

		return &TreeNode{
			Rule:     ruleName,
			Start:    [2]int{startLine, startCol},
			End:      [2]int{endLine, endCol},
			Children: children,
		}
	}
	return nil
}

// tokenDisplayName returns the display name for a token type.
func tokenDisplayName(tokenType int, symbolicNames, literalNames []string) string {
	if tokenType >= 0 && tokenType < len(literalNames) && literalNames[tokenType] != "" {
		return literalNames[tokenType]
	}
	if tokenType >= 0 && tokenType < len(symbolicNames) && symbolicNames[tokenType] != "" {
		return symbolicNames[tokenType]
	}
	return fmt.Sprintf("%d", tokenType)
}
