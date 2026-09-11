package objc

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
*/
import "C"

import "unsafe"

// Language returns the tree-sitter language pointer for Objective-C.
func Language() unsafe.Pointer { return unsafe.Pointer(C.tree_sitter_objc()) }
