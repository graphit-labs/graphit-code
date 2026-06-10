package dart

import (
	tree_sitter_dart "github.com/UserNobody14/tree-sitter-dart/bindings/go"
	sitter "github.com/smacker/go-tree-sitter"
)

func GetLanguage() *sitter.Language {
	return sitter.NewLanguage(tree_sitter_dart.Language())
}
