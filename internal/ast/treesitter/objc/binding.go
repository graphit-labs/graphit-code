package objc

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
*/
import "C"

import (
	"unsafe"

	sitter "github.com/smacker/go-tree-sitter"
)

// GetLanguage returns the tree-sitter language for Objective-C.
func GetLanguage() *sitter.Language {
	ptr := unsafe.Pointer(C.tree_sitter_objc())
	return sitter.NewLanguage(ptr)
}
