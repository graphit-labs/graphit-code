package clojure

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
*/
import "C"

import (
	"unsafe"

	sitter "github.com/smacker/go-tree-sitter"
)

// GetLanguage returns the tree-sitter language for Clojure.
func GetLanguage() *sitter.Language {
	ptr := unsafe.Pointer(C.tree_sitter_clojure())
	return sitter.NewLanguage(ptr)
}
