// Package wasmts provides a CGO-free tree-sitter integration via wazero.
// It loads tree-sitter compiled to WASM and communicates with the C ABI
// through exported functions, enabling plug-and-play grammar loading.
package wasmts

// WASM exported function names — these are the tree-sitter C API symbols
// exported by the compiled .wasm modules.
const (
	// Memory management (libc)
	_malloc = "malloc"
	_free   = "free"
	_strlen = "strlen"

	// Parser
	_parserNew         = "ts_parser_new"
	_parserDelete      = "ts_parser_delete"
	_parserSetLanguage = "ts_parser_set_language"
	_parserParseString = "ts_parser_parse_string"

	// Tree
	_treeRootNode = "ts_tree_root_node"
	_treeDelete   = "ts_tree_delete"

	// Query
	_queryNew              = "ts_query_new"
	_queryDelete           = "ts_query_delete"
	_queryCursorNew        = "ts_query_cursor_new"
	_queryCursorDelete     = "ts_query_cursor_delete"
	_queryCursorExec       = "ts_query_cursor_exec"
	_queryCursorNextMatch  = "ts_query_cursor_next_match"
	_queryCaptureNameForID = "ts_query_capture_name_for_id"

	// Node — basic
	_nodeType           = "ts_node_type"
	_nodeString         = "ts_node_string"
	_nodeChildCount     = "ts_node_child_count"
	_nodeNamedChildCount = "ts_node_named_child_count"
	_nodeChild          = "ts_node_child"
	_nodeNamedChild     = "ts_node_named_child"
	_nodeStartByte      = "ts_node_start_byte"
	_nodeEndByte        = "ts_node_end_byte"
	_nodeIsError        = "ts_node_is_error"
	_nodeIsNull         = "ts_node_is_null"

	// Node — extended (not in malivvan, but needed by graphit)
	_nodeParent           = "ts_node_parent"
	_nodeChildByFieldName = "ts_node_child_by_field_name"
	_nodeStartPoint       = "ts_node_start_point"
	_nodeEndPoint         = "ts_node_end_point"

	// Language
	_languageVersion = "ts_language_version"
)

// _coreFunctions lists all exported functions we require from the core WASM module.
// Used for validation during module loading.
var _coreFunctions = []string{
	_malloc, _free, _strlen,
	_parserNew, _parserDelete, _parserSetLanguage, _parserParseString,
	_treeRootNode, _treeDelete,
	_queryNew, _queryDelete,
	_queryCursorNew, _queryCursorDelete, _queryCursorExec, _queryCursorNextMatch,
	_queryCaptureNameForID,
	_nodeType, _nodeString,
	_nodeChildCount, _nodeNamedChildCount,
	_nodeChild, _nodeNamedChild,
	_nodeStartByte, _nodeEndByte,
	_nodeIsError, _nodeIsNull,
	_nodeParent, _nodeChildByFieldName,
	_nodeStartPoint, _nodeEndPoint,
	_languageVersion,
}

// TSNode is 24 bytes in the WASM memory layout (tree-sitter internal struct).
const tsNodeSize = 24

// TSQueryMatch is 12 bytes in WASM memory:
//   uint32 id (offset 0)
//   uint16 pattern_index (offset 4)
//   uint16 capture_count (offset 6)
//   uint32 captures_ptr (offset 8)
const tsQueryMatchSize = 12

// TSQueryCapture is 28 bytes:
//   TSNode node (24 bytes, offset 0)
//   uint32 index (offset 24)
const tsQueryCaptureSize = 28

// TSPoint is 8 bytes:
//   uint32 row (offset 0)
//   uint32 column (offset 4)
const tsPointSize = 8
