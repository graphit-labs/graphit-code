package julia

import (
	tree_sitter_julia "github.com/tree-sitter/tree-sitter-julia/bindings/go"
	sitter "github.com/smacker/go-tree-sitter"
)

func GetLanguage() *sitter.Language {
	return sitter.NewLanguage(tree_sitter_julia.Language())
}
