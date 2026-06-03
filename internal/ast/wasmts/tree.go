package wasmts

import (
	"fmt"
)

// Tree represents a parsed syntax tree.
// It retains the original source bytes for Content() lookups on nodes.
type Tree struct {
	lang *Language
	ptr  uint64
	src  []byte // original source code
}

// RootNode returns the root node of the syntax tree.
func (t *Tree) RootNode() (*Node, error) {
	// Allocate space for TSNode (24 bytes) in WASM memory
	nodePtr, err := t.lang.module.allocateBytes(tsNodeSize)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate root node: %w", err)
	}

	// ts_tree_root_node(outNode, tree) — the WASM ABI returns the struct
	// via an output pointer (first arg) since TSNode is a value type.
	_, err = t.lang.module.call(_treeRootNode, nodePtr, t.ptr)
	if err != nil {
		t.lang.module.freePtr(nodePtr)
		return nil, fmt.Errorf("wasmts: ts_tree_root_node: %w", err)
	}

	return &Node{
		lang: t.lang,
		ptr:  nodePtr,
		src:  t.src,
	}, nil
}

// Close releases the tree from WASM memory.
func (t *Tree) Close() {
	if t.ptr != 0 {
		t.lang.module.call(_treeDelete, t.ptr) //nolint:errcheck
		t.ptr = 0
	}
}
