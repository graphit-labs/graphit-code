package json

import (
	tree_sitter_json "github.com/tree-sitter/tree-sitter-json/bindings/go"
	sitter "github.com/smacker/go-tree-sitter"
)

func GetLanguage() *sitter.Language {
	return sitter.NewLanguage(tree_sitter_json.Language())
}
