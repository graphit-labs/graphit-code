package ast

import (
	"fmt"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// tsParse bridges the official binding's ParseCtx (which returns only *Tree)
// to the (*Tree, error) shape used throughout the tests in this package.
func tsParse(p *sitter.Parser, src []byte) (*sitter.Tree, error) {
	tree := p.Parse(src, nil)
	if tree == nil {
		return nil, fmt.Errorf("tree-sitter parse returned nil")
	}
	return tree, nil
}
