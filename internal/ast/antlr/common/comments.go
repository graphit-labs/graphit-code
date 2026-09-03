package antlrcommon

import (
	"strings"

	"github.com/antlr4-go/antlr/v4"
)

// CollectComments returns the comment tokens the parser never sees.
//
// Every driver builds its stream with antlr.NewCommonTokenStream(lexer,
// antlr.TokenDefaultChannel), and these grammars send comments to the hidden
// channel, so comments are absent from the parse tree entirely. They are not
// lost, though: CommonTokenStream buffers every token the lexer produced and
// merely filters by channel on access, so after the parse they can still be
// read back.
//
// The result is attached to the root as TreeNode.Comments rather than spliced
// into Children. Children is what the extraction patterns walk, and injecting
// nodes there would change what every existing pattern matches. A separate field
// still crosses the sidecar's JSON protocol on its own.
//
// Fill is called first because the parser may have stopped before EOF — on a
// parse error, or simply because the grammar's entry rule completed — and the
// trailing tokens would otherwise never have been pulled from the lexer.
func CollectComments(tokens *antlr.CommonTokenStream, symbolicNames []string) []*TreeNode {
	if tokens == nil {
		return nil
	}
	tokens.Fill()

	var out []*TreeNode
	for _, tok := range tokens.GetAllTokens() {
		if tok == nil || tok.GetChannel() == antlr.LexerDefaultTokenChannel {
			continue
		}
		if !isCommentToken(tok, symbolicNames) {
			continue
		}
		text := tok.GetText()
		if strings.TrimSpace(text) == "" {
			continue
		}
		line := tok.GetLine()
		out = append(out, &TreeNode{
			Token: tokenDisplayName(tok.GetTokenType(), symbolicNames, nil),
			Text:  text,
			Start: [2]int{line, tok.GetColumn()},
			End:   [2]int{line + strings.Count(text, "\n"), tok.GetColumn() + len(text) - 1},
		})
	}
	return out
}

func isCommentToken(tok antlr.Token, symbolicNames []string) bool {
	t := tok.GetTokenType()
	if t < 0 || t >= len(symbolicNames) {
		return false
	}
	return strings.Contains(strings.ToUpper(symbolicNames[t]), "COMMENT")
}
