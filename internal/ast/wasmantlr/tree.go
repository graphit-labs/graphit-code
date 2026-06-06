package wasmantlr

import "encoding/json"

// TreeNode represents a node in the ANTLR parse tree serialized as JSON.
// Internal (rule) nodes have Rule set; leaf (terminal) nodes have Token+Text.
type TreeNode struct {
	Rule     string      `json:"rule,omitempty"`
	Token    string      `json:"token,omitempty"`
	Text     string      `json:"text,omitempty"`
	Start    [2]int      `json:"start"`    // [line (1-indexed), column (0-indexed)]
	End      [2]int      `json:"end"`      // [line (1-indexed), column (0-indexed)]
	Children []*TreeNode `json:"children,omitempty"`
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

// ChildByRule returns the first child with the given rule name, or nil.
func (n *TreeNode) ChildByRule(rule string) *TreeNode {
	for _, child := range n.Children {
		if child.Rule == rule {
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

// ParseTreeFromJSON deserializes a JSON parse tree from ANTLR WASM output.
func ParseTreeFromJSON(data []byte) (*TreeNode, error) {
	var root TreeNode
	if err := json.Unmarshal(data, &root); err != nil {
		return nil, err
	}
	return &root, nil
}
