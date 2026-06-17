package haskell

import (
	tree_sitter_haskell "github.com/tree-sitter/tree-sitter-haskell/bindings/go"
	sitter "github.com/smacker/go-tree-sitter"
)

func GetLanguage() *sitter.Language {
	return sitter.NewLanguage(tree_sitter_haskell.Language())
}
