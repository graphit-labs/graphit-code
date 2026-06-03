package wasmts

import (
	"fmt"
)

// Node represents a tree-sitter syntax tree node.
// It wraps a pointer to a TSNode struct (24 bytes) in WASM linear memory.
// Nodes hold a reference to the original source for Content() lookups.
type Node struct {
	lang *Language
	ptr  uint64
	src  []byte // shared reference to original source
}

// Point represents a position in the source code.
type Point struct {
	Row    uint32
	Column uint32
}

// --- Basic node properties ---

// Type returns the node's grammar type (e.g., "function_declaration").
// Equivalent to smacker's node.Type().
func (n *Node) Type() (string, error) {
	result, err := n.lang.module.call(_nodeType, n.ptr)
	if err != nil {
		return "", fmt.Errorf("wasmts: ts_node_type: %w", err)
	}
	return n.lang.module.readString(result[0])
}

// IsNull checks if the node is a null/empty node.
func (n *Node) IsNull() (bool, error) {
	result, err := n.lang.module.call(_nodeIsNull, n.ptr)
	if err != nil {
		return true, fmt.Errorf("wasmts: ts_node_is_null: %w", err)
	}
	return result[0] != 0, nil
}

// IsError returns true if the node represents a parse error.
func (n *Node) IsError() (bool, error) {
	result, err := n.lang.module.call(_nodeIsError, n.ptr)
	if err != nil {
		return false, fmt.Errorf("wasmts: ts_node_is_error: %w", err)
	}
	return result[0] != 0, nil
}

// --- Position ---

// StartByte returns the byte offset where this node starts in the source.
func (n *Node) StartByte() (uint32, error) {
	result, err := n.lang.module.call(_nodeStartByte, n.ptr)
	if err != nil {
		return 0, fmt.Errorf("wasmts: ts_node_start_byte: %w", err)
	}
	return uint32(result[0]), nil
}

// EndByte returns the byte offset where this node ends in the source.
func (n *Node) EndByte() (uint32, error) {
	result, err := n.lang.module.call(_nodeEndByte, n.ptr)
	if err != nil {
		return 0, fmt.Errorf("wasmts: ts_node_end_byte: %w", err)
	}
	return uint32(result[0]), nil
}

// StartPoint returns the row and column where this node starts.
// Equivalent to smacker's node.StartPoint().Row.
func (n *Node) StartPoint() (Point, error) {
	// ts_node_start_point returns TSPoint (8 bytes: row u32 + col u32)
	// In WASM, struct return values are via output pointer.
	pointPtr, err := n.lang.module.allocateBytes(tsPointSize)
	if err != nil {
		return Point{}, fmt.Errorf("wasmts: allocate start point: %w", err)
	}
	defer n.lang.module.freePtr(pointPtr)

	_, err = n.lang.module.call(_nodeStartPoint, pointPtr, n.ptr)
	if err != nil {
		return Point{}, fmt.Errorf("wasmts: ts_node_start_point: %w", err)
	}

	row, ok := n.lang.module.mod.Memory().ReadUint32Le(uint32(pointPtr))
	if !ok {
		return Point{}, fmt.Errorf("wasmts: read start point row")
	}
	col, ok := n.lang.module.mod.Memory().ReadUint32Le(uint32(pointPtr) + 4)
	if !ok {
		return Point{}, fmt.Errorf("wasmts: read start point col")
	}

	return Point{Row: row, Column: col}, nil
}

// EndPoint returns the row and column where this node ends.
func (n *Node) EndPoint() (Point, error) {
	pointPtr, err := n.lang.module.allocateBytes(tsPointSize)
	if err != nil {
		return Point{}, fmt.Errorf("wasmts: allocate end point: %w", err)
	}
	defer n.lang.module.freePtr(pointPtr)

	_, err = n.lang.module.call(_nodeEndPoint, pointPtr, n.ptr)
	if err != nil {
		return Point{}, fmt.Errorf("wasmts: ts_node_end_point: %w", err)
	}

	row, ok := n.lang.module.mod.Memory().ReadUint32Le(uint32(pointPtr))
	if !ok {
		return Point{}, fmt.Errorf("wasmts: read end point row")
	}
	col, ok := n.lang.module.mod.Memory().ReadUint32Le(uint32(pointPtr) + 4)
	if !ok {
		return Point{}, fmt.Errorf("wasmts: read end point col")
	}

	return Point{Row: row, Column: col}, nil
}

// --- Content ---

// Content returns the source text corresponding to this node.
// This is a pure Go operation — no WASM call needed.
// Equivalent to smacker's node.Content(src).
func (n *Node) Content() string {
	start, err := n.StartByte()
	if err != nil {
		return ""
	}
	end, err := n.EndByte()
	if err != nil {
		return ""
	}
	if int(start) >= len(n.src) || int(end) > len(n.src) || start > end {
		return ""
	}
	return string(n.src[start:end])
}

// --- Children ---

// ChildCount returns the number of children.
func (n *Node) ChildCount() (uint32, error) {
	result, err := n.lang.module.call(_nodeChildCount, n.ptr)
	if err != nil {
		return 0, fmt.Errorf("wasmts: ts_node_child_count: %w", err)
	}
	return uint32(result[0]), nil
}

// NamedChildCount returns the number of named children.
func (n *Node) NamedChildCount() (uint32, error) {
	result, err := n.lang.module.call(_nodeNamedChildCount, n.ptr)
	if err != nil {
		return 0, fmt.Errorf("wasmts: ts_node_named_child_count: %w", err)
	}
	return uint32(result[0]), nil
}

// Child returns the child at the given index.
func (n *Node) Child(index int) (*Node, error) {
	childPtr, err := n.lang.module.allocateBytes(tsNodeSize)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate child node: %w", err)
	}

	_, err = n.lang.module.call(_nodeChild, childPtr, n.ptr, uint64(index))
	if err != nil {
		n.lang.module.freePtr(childPtr)
		return nil, fmt.Errorf("wasmts: ts_node_child: %w", err)
	}

	child := &Node{lang: n.lang, ptr: childPtr, src: n.src}

	// Check if the returned node is null
	isNull, err := child.IsNull()
	if err != nil || isNull {
		n.lang.module.freePtr(childPtr)
		return nil, nil
	}

	return child, nil
}

// NamedChild returns the named child at the given index.
func (n *Node) NamedChild(index int) (*Node, error) {
	childPtr, err := n.lang.module.allocateBytes(tsNodeSize)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate named child node: %w", err)
	}

	_, err = n.lang.module.call(_nodeNamedChild, childPtr, n.ptr, uint64(index))
	if err != nil {
		n.lang.module.freePtr(childPtr)
		return nil, fmt.Errorf("wasmts: ts_node_named_child: %w", err)
	}

	child := &Node{lang: n.lang, ptr: childPtr, src: n.src}
	isNull, err := child.IsNull()
	if err != nil || isNull {
		n.lang.module.freePtr(childPtr)
		return nil, nil
	}

	return child, nil
}

// --- Parent ---

// Parent returns the parent node, or nil if this is the root.
// Equivalent to smacker's node.Parent().
func (n *Node) Parent() (*Node, error) {
	parentPtr, err := n.lang.module.allocateBytes(tsNodeSize)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate parent node: %w", err)
	}

	_, err = n.lang.module.call(_nodeParent, parentPtr, n.ptr)
	if err != nil {
		n.lang.module.freePtr(parentPtr)
		return nil, fmt.Errorf("wasmts: ts_node_parent: %w", err)
	}

	parent := &Node{lang: n.lang, ptr: parentPtr, src: n.src}
	isNull, err := parent.IsNull()
	if err != nil || isNull {
		n.lang.module.freePtr(parentPtr)
		return nil, nil
	}

	return parent, nil
}

// --- Field access ---

// ChildByFieldName returns the child with the given field name, or nil.
// Equivalent to smacker's node.ChildByFieldName("name").
func (n *Node) ChildByFieldName(name string) (*Node, error) {
	childPtr, err := n.lang.module.allocateBytes(tsNodeSize)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate field child: %w", err)
	}

	namePtr, nameLen, freeName, err := n.lang.module.allocateString(name)
	if err != nil {
		n.lang.module.freePtr(childPtr)
		return nil, fmt.Errorf("wasmts: allocate field name: %w", err)
	}
	defer freeName()

	// ts_node_child_by_field_name(outNode, node, name, name_length)
	_, err = n.lang.module.call(_nodeChildByFieldName, childPtr, n.ptr, namePtr, nameLen)
	if err != nil {
		n.lang.module.freePtr(childPtr)
		return nil, fmt.Errorf("wasmts: ts_node_child_by_field_name: %w", err)
	}

	child := &Node{lang: n.lang, ptr: childPtr, src: n.src}
	isNull, err := child.IsNull()
	if err != nil || isNull {
		n.lang.module.freePtr(childPtr)
		return nil, nil
	}

	return child, nil
}

// --- Debug ---

// String returns the S-expression representation of this node.
func (n *Node) String() (string, error) {
	result, err := n.lang.module.call(_nodeString, n.ptr)
	if err != nil {
		return "", fmt.Errorf("wasmts: ts_node_string: %w", err)
	}
	s, err := n.lang.module.readString(result[0])
	if err != nil {
		return "", err
	}
	// ts_node_string allocates, so we should free it
	n.lang.module.freePtr(result[0])
	return s, nil
}
