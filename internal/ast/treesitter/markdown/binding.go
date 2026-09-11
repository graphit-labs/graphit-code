package markdown

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
#include "scanner.c.inc"
*/
import "C"

import "unsafe"

// Language returns the tree-sitter language pointer for the Markdown block grammar.
func Language() unsafe.Pointer { return unsafe.Pointer(C.tree_sitter_markdown()) }
