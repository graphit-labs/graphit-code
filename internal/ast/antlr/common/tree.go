package antlrcommon

import "strings"

// TreeNode represents a node in the ANTLR parse tree serialized as JSON.
// Internal (rule) nodes have Rule set; leaf (terminal) nodes have Token+Text.
type TreeNode struct {
	Rule     string      `json:"rule,omitempty"`
	Token    string      `json:"token,omitempty"`
	Text     string      `json:"text,omitempty"`
	Start    [2]int      `json:"start"` // [line (1-indexed), column (0-indexed)]
	End      [2]int      `json:"end"`   // [line (1-indexed), column (0-indexed)]
	Children []*TreeNode `json:"children,omitempty"`

	// Comments holds the hidden-channel comment tokens, set on the root only.
	// Kept out of Children so the extraction patterns that walk the tree are
	// unaffected; see CollectComments.
	Comments []*TreeNode `json:"comments,omitempty"`
}

// IsTerminal returns true if this is a leaf/token node.
func (n *TreeNode) IsTerminal() bool {
	return n.Token != ""
}

// IsRule returns true if this is an internal/rule node.
func (n *TreeNode) IsRule() bool {
	return n.Rule != ""
}

// StartLine returns the 1-indexed start line.
func (n *TreeNode) StartLine() int { return n.Start[0] }

// EndLine returns the 1-indexed end line.
func (n *TreeNode) EndLine() int { return n.End[0] }

// FirstTerminalText returns the text of the first terminal descendant.
// Used to extract entity names from rule nodes.
func (n *TreeNode) FirstTerminalText() string {
	if n.IsTerminal() {
		return n.Text
	}
	for _, child := range n.Children {
		if text := child.FirstTerminalText(); text != "" {
			return text
		}
	}
	return ""
}

// QualifiedNameText returns the object name a name node carries, dropping any
// qualifier in front of it: "GC"."ABERTURA_CAIXA_LOJA" is the table
// ABERTURA_CAIXA_LOJA, not the schema GC.
//
// FirstTerminalText cannot answer this. The SQL grammars spell a qualified name
// as `identifier ('.' id_expression)*`, so its first terminal is the leftmost
// component — the schema. Reading names that way made every Oracle sequence in a
// 35k-file export come out named GC, pointed every DML edge at a single node
// called GC, and attributed every column to a table of that name.
//
// Returns "" when the node is not shaped like a qualified name — a quoted string,
// a statement keyword, an expression — so callers keep their previous behaviour
// by falling back to FirstTerminalText.
func (n *TreeNode) QualifiedNameText() string {
	return n.dottedName(false)
}

// DeclaredNameText returns the name a declaration node declares: the object name
// in `CREATE TABLE "GC"."X"`, `CREATE OR REPLACE VIEW "GC"."V"`, `FD fileName`.
//
// Unlike QualifiedNameText it walks past the leading keywords and past clauses
// that precede the name (OR REPLACE, editioning_clause), and descends one level
// when the declaration delegates its name to a single child rule — which is how
// `create_type: CREATE TYPE (type_definition | type_body)` hides type_name.
//
// Returns "" when no name-shaped child exists.
func (n *TreeNode) DeclaredNameText() string {
	if name := n.dottedName(true); name != "" {
		return name
	}
	for _, child := range n.Children {
		if child.IsRule() {
			return child.dottedName(true)
		}
	}
	return ""
}

// dottedName reads the direct children as a qualified name and returns its last
// component. With skipLeading it ignores keywords and unrelated clauses until the
// name starts, which is what a declaration node needs; without it the node itself
// must be the name.
func (n *TreeNode) dottedName(skipLeading bool) string {
	last := ""
	afterDot := false
	for _, child := range n.Children {
		if child.IsTerminal() {
			if child.Text == "." && last != "" {
				afterDot = true
				continue
			}
			// A keyword or a delimiter: it ends the name if one has started,
			// and precedes it otherwise.
			if last != "" || !skipLeading {
				break
			}
			continue
		}
		if !isNameRule(child.Rule) {
			if last != "" || !skipLeading {
				break
			}
			continue
		}
		// Two identifiers side by side with no '.' between them are two
		// different names (SYNONYM x FOR y), not one qualified name.
		if last != "" && !afterDot {
			break
		}
		if part := child.componentText(); part != "" {
			last = part
		}
		afterDot = false
	}
	return last
}

// componentText is the text of one component of a qualified name. The component
// may itself be qualified — function_name is `identifier ('.' id_expression)?` —
// so it resolves recursively before falling back to the leading terminal.
func (n *TreeNode) componentText() string {
	if n.IsTerminal() {
		return n.Text
	}
	if inner := n.dottedName(false); inner != "" {
		return inner
	}
	return n.FirstTerminalText()
}

// isNameRule reports whether a rule node can hold one component of a name. The
// grammars in this family name those rules "<thing>_name" or "<thing>Name", plus
// the generic identifier rules every SQL grammar shares.
func isNameRule(rule string) bool {
	switch rule {
	case "identifier", "id_expression", "regular_id":
		return true
	}
	return strings.HasSuffix(rule, "_name") || strings.HasSuffix(rule, "Name")
}

// ChildByRule returns the first child with the given rule name, or nil.
// ChildByRule finds a direct child by its rule name, falling back to its TOKEN
// name.
//
// The fallback exists because a grammar states some facts as a keyword rather than
// as a rule, and those facts are not reachable any other way. Oracle's index is the
// plain case:
//
//	create_index : CREATE UNIQUE? BITMAP? INDEX index_name ON ...
//
// Uniqueness — the difference between "the database enforces this" and "the
// application hopes so" — is the presence of one terminal. With rules only, a query
// could name every part of that statement except the part that matters.
//
// Rule first, so a grammar with a rule and a token of the same name keeps the
// behaviour it had; only a lookup that would have returned nothing can now match a
// token.
func (n *TreeNode) ChildByRule(rule string) *TreeNode {
	for _, child := range n.Children {
		if child.Rule == rule {
			return child
		}
	}
	for _, child := range n.Children {
		// A terminal's Token is the grammar's spelling of it, and a literal keyword
		// is spelled quoted — `'UNIQUE'`, not `UNIQUE`. Callers write the bare word,
		// which is also how the keyword guards in the query patterns spell it.
		if strings.Trim(child.Token, "'") == rule {
			return child
		}
	}
	return nil
}

// FullText reconstructs the full text content of this subtree by
// concatenating all terminal descendants with spaces.
func (n *TreeNode) FullText() string {
	if n.IsTerminal() {
		return n.Text
	}
	var buf []byte
	n.collectText(&buf)
	return string(buf)
}

func (n *TreeNode) collectText(buf *[]byte) {
	if n.IsTerminal() {
		if len(*buf) > 0 {
			*buf = append(*buf, ' ')
		}
		*buf = append(*buf, n.Text...)
		return
	}
	for _, child := range n.Children {
		child.collectText(buf)
	}
}
