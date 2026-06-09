package treesitter

/*
#cgo CFLAGS: -I../../../internal/ast/wasmts/csrc/ -I../../../internal/ast/wasmts/csrc/tree_sitter/
#include <api.h>
#include <stdlib.h>

extern const TSLanguage *tree_sitter_c(void);
extern const TSLanguage *tree_sitter_cpp(void);
extern const TSLanguage *tree_sitter_c_sharp(void);
extern const TSLanguage *tree_sitter_dart(void);
extern const TSLanguage *tree_sitter_go(void);
extern const TSLanguage *tree_sitter_java(void);
extern const TSLanguage *tree_sitter_javascript(void);
extern const TSLanguage *tree_sitter_kotlin(void);
extern const TSLanguage *tree_sitter_php(void);
extern const TSLanguage *tree_sitter_python(void);
extern const TSLanguage *tree_sitter_ruby(void);
extern const TSLanguage *tree_sitter_rust(void);
extern const TSLanguage *tree_sitter_sql(void);
extern const TSLanguage *tree_sitter_swift(void);
extern const TSLanguage *tree_sitter_typescript(void);
extern const TSLanguage *tree_sitter_tsx(void);
extern const TSLanguage *tree_sitter_xml(void);
*/
import "C"
import (
	"fmt"
	"unsafe"
)

func GetLanguage(name string) (unsafe.Pointer, error) {
	switch name {
	case "c":
		return unsafe.Pointer(C.tree_sitter_c()), nil
	case "cpp":
		return unsafe.Pointer(C.tree_sitter_cpp()), nil
	case "c_sharp", "csharp":
		return unsafe.Pointer(C.tree_sitter_c_sharp()), nil
	case "dart":
		return unsafe.Pointer(C.tree_sitter_dart()), nil
	case "go", "golang":
		return unsafe.Pointer(C.tree_sitter_go()), nil
	case "java":
		return unsafe.Pointer(C.tree_sitter_java()), nil
	case "javascript":
		return unsafe.Pointer(C.tree_sitter_javascript()), nil
	case "kotlin":
		return unsafe.Pointer(C.tree_sitter_kotlin()), nil
	case "php":
		return unsafe.Pointer(C.tree_sitter_php()), nil
	case "python":
		return unsafe.Pointer(C.tree_sitter_python()), nil
	case "ruby":
		return unsafe.Pointer(C.tree_sitter_ruby()), nil
	case "rust":
		return unsafe.Pointer(C.tree_sitter_rust()), nil
	case "sql":
		return unsafe.Pointer(C.tree_sitter_sql()), nil
	case "swift":
		return unsafe.Pointer(C.tree_sitter_swift()), nil
	case "typescript":
		return unsafe.Pointer(C.tree_sitter_typescript()), nil
	case "tsx":
		return unsafe.Pointer(C.tree_sitter_tsx()), nil
	case "xml":
		return unsafe.Pointer(C.tree_sitter_xml()), nil
	default:
		return nil, fmt.Errorf("unknown grammar language: %s", name)
	}
}

type Parser struct {
	ptr *C.TSParser
}

func NewParser() *Parser {
	return &Parser{ptr: C.ts_parser_new()}
}

func (p *Parser) Close() {
	if p.ptr != nil {
		C.ts_parser_delete(p.ptr)
		p.ptr = nil
	}
}

func (p *Parser) SetLanguage(lang unsafe.Pointer) bool {
	return bool(C.ts_parser_set_language(p.ptr, (*C.TSLanguage)(lang)))
}

func (p *Parser) Parse(source []byte) (*Tree, error) {
	var srcPtr *C.char
	if len(source) > 0 {
		srcPtr = (*C.char)(unsafe.Pointer(&source[0]))
	}
	tPtr := C.ts_parser_parse_string(p.ptr, nil, srcPtr, C.uint32_t(len(source)))
	if tPtr == nil {
		return nil, fmt.Errorf("failed to parse")
	}
	return &Tree{ptr: tPtr, source: source}, nil
}

type Tree struct {
	ptr    *C.TSTree
	source []byte
}

func (t *Tree) Close() {
	if t.ptr != nil {
		C.ts_tree_delete(t.ptr)
		t.ptr = nil
	}
}

func (t *Tree) RootNode() Node {
	cNode := C.ts_tree_root_node(t.ptr)
	return Node{cNode: cNode, source: t.source}
}

type Node struct {
	cNode  C.TSNode
	source []byte
}

func (n Node) Type() string {
	return C.GoString(C.ts_node_type(n.cNode))
}

func (n Node) IsNull() bool {
	return bool(C.ts_node_is_null(n.cNode))
}

func (n Node) IsError() bool {
	return bool(C.ts_node_is_error(n.cNode))
}

func (n Node) StartByte() uint32 {
	return uint32(C.ts_node_start_byte(n.cNode))
}

func (n Node) EndByte() uint32 {
	return uint32(C.ts_node_end_byte(n.cNode))
}

type Point struct {
	Row    uint32
	Column uint32
}

func (n Node) StartPoint() Point {
	pt := C.ts_node_start_point(n.cNode)
	return Point{Row: uint32(pt.row), Column: uint32(pt.column)}
}

func (n Node) EndPoint() Point {
	pt := C.ts_node_end_point(n.cNode)
	return Point{Row: uint32(pt.row), Column: uint32(pt.column)}
}

func (n Node) Content() string {
	start := n.StartByte()
	end := n.EndByte()
	if int(start) >= len(n.source) || int(end) > len(n.source) || start > end {
		return ""
	}
	return string(n.source[start:end])
}

func (n Node) ChildCount() uint32 {
	return uint32(C.ts_node_child_count(n.cNode))
}

func (n Node) Child(index int) Node {
	cChild := C.ts_node_child(n.cNode, C.uint32_t(index))
	return Node{cNode: cChild, source: n.source}
}

func (n Node) Parent() Node {
	cParent := C.ts_node_parent(n.cNode)
	return Node{cNode: cParent, source: n.source}
}

func (n Node) ChildByFieldName(name string) Node {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	cChild := C.ts_node_child_by_field_name(n.cNode, cName, C.uint32_t(len(name)))
	return Node{cNode: cChild, source: n.source}
}

type Query struct {
	ptr *C.TSQuery
}

func NewQuery(lang unsafe.Pointer, pattern string) (*Query, error) {
	cPattern := C.CString(pattern)
	defer C.free(unsafe.Pointer(cPattern))
	var errorOffset C.uint32_t
	var errorType C.TSQueryError

	qPtr := C.ts_query_new(
		(*C.TSLanguage)(lang),
		cPattern,
		C.uint32_t(len(pattern)),
		&errorOffset,
		&errorType,
	)
	if qPtr == nil {
		return nil, fmt.Errorf("failed to compile query at offset %d (error type %d)", errorOffset, errorType)
	}
	return &Query{ptr: qPtr}, nil
}

func (q *Query) Close() {
	if q.ptr != nil {
		C.ts_query_delete(q.ptr)
		q.ptr = nil
	}
}

type QueryCursor struct {
	ptr *C.TSQueryCursor
}

func NewQueryCursor() *QueryCursor {
	return &QueryCursor{ptr: C.ts_query_cursor_new()}
}

func (qc *QueryCursor) Close() {
	if qc.ptr != nil {
		C.ts_query_cursor_delete(qc.ptr)
		qc.ptr = nil
	}
}

func (qc *QueryCursor) Exec(q *Query, n Node) {
	C.ts_query_cursor_exec(qc.ptr, q.ptr, n.cNode)
}

type QueryCapture struct {
	Node  Node
	Index uint32
}

type QueryMatch struct {
	ID           uint32
	PatternIndex uint16
	Captures     []QueryCapture
}

func (qc *QueryCursor) NextMatch(q *Query, source []byte) (*QueryMatch, bool) {
	var cMatch C.TSQueryMatch
	if !bool(C.ts_query_cursor_next_match(qc.ptr, &cMatch)) { //nolint:gocritic // CGO pointer checks trigger dupSubExpr
		return nil, false
	}

	capturesCount := int(cMatch.capture_count)
	captures := make([]QueryCapture, capturesCount)
	cCaptures := unsafe.Slice(cMatch.captures, capturesCount)
	for i := 0; i < capturesCount; i++ {
		captures[i] = QueryCapture{
			Node:  Node{cNode: cCaptures[i].node, source: source},
			Index: uint32(cCaptures[i].index),
		}
	}

	return &QueryMatch{
		ID:           uint32(cMatch.id),
		PatternIndex: uint16(cMatch.pattern_index),
		Captures:     captures,
	}, true
}
