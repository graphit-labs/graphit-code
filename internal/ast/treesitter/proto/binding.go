package proto

/*
#cgo CFLAGS: -std=c11 -fPIC
#include "parser.c.inc"
*/
import "C"

import "unsafe"

// Language returns the tree-sitter language pointer for Protocol Buffers.
func Language() unsafe.Pointer { return unsafe.Pointer(C.tree_sitter_proto()) }
