package r

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
#include "scanner.c.inc"
*/
import "C"

import (
	"unsafe"

	sitter "github.com/smacker/go-tree-sitter"
)

// GetLanguage returns the tree-sitter language for R.
func GetLanguage() *sitter.Language {
	ptr := unsafe.Pointer(C.tree_sitter_r())
	return sitter.NewLanguage(ptr)
}
