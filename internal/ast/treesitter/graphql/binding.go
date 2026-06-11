package graphql

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
*/
import "C"

import (
	"unsafe"

	sitter "github.com/smacker/go-tree-sitter"
)

// GetLanguage returns the tree-sitter language for GraphQL.
func GetLanguage() *sitter.Language {
	ptr := unsafe.Pointer(C.tree_sitter_graphql())
	return sitter.NewLanguage(ptr)
}
