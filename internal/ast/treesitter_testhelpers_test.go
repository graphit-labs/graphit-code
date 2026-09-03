package ast

import (
	"fmt"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

func tsParse(p *sitter.Parser, src []byte) (*sitter.Tree, error) {
	tree := p.Parse(src, nil)
	if tree == nil {
		return nil, fmt.Errorf("tree-sitter parse returned nil")
	}
	return tree, nil
}
