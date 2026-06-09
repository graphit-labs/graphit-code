package wasmts

import (
	"errors"
	"fmt"
	"regexp"
	"strings"
)

// Query represents a compiled tree-sitter query pattern.
type Query struct {
	lang *Language
	ptr  uint64
}

// QueryCursor iterates over query matches on a syntax tree.
type QueryCursor struct {
	lang *Language
	ptr  uint64
}

// QueryMatch represents a single match from a query execution.
type QueryMatch struct {
	ID           uint32
	PatternIndex uint16
	Captures     []QueryCapture
}

// QueryCapture represents a single captured node in a query match.
type QueryCapture struct {
	ID   uint32
	Node *Node
}

// Query error types
const (
	QueryErrorNone      uint32 = 0
	QueryErrorSyntax    uint32 = 1
	QueryErrorNodeType  uint32 = 2
	QueryErrorField     uint32 = 3
	QueryErrorCapture   uint32 = 4
	QueryErrorStructure uint32 = 5
	QueryErrorLanguage  uint32 = 6
)

// NewQuery compiles a tree-sitter query pattern.
// Equivalent to smacker's sitter.NewQuery([]byte(pattern), lang).
// NewQuery compiles and caches a tree-sitter query pattern.
// Equivalent to smacker's sitter.NewQuery([]byte(pattern), lang).
func (l *Language) NewQuery(pattern string) (*Query, error) {
	if l.queries == nil {
		l.queries = make(map[string]*Query)
	}
	if q, ok := l.queries[pattern]; ok {
		return q, nil
	}

	q, err := l.compileQueryImpl(pattern)
	if err != nil {
		return nil, err
	}
	l.queries[pattern] = q
	return q, nil
}

func (l *Language) compileQueryImpl(pattern string) (*Query, error) {
	// Allocate error output pointers (2x uint32)
	errOffPtr, err := l.module.allocateBytes(4)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate error offset: %w", err)
	}
	defer l.module.freePtr(errOffPtr)

	errTypePtr, err := l.module.allocateBytes(4)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate error type: %w", err)
	}
	defer l.module.freePtr(errTypePtr)

	patternPtr, patternSize, freePattern, err := l.module.allocateString(pattern)
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate pattern: %w", err)
	}
	defer freePattern()

	// ts_query_new(language, source, source_len, error_offset, error_type)
	result, err := l.module.call(_queryNew, l.ptr, patternPtr, patternSize, errOffPtr, errTypePtr)
	if err != nil {
		return nil, fmt.Errorf("wasmts: ts_query_new: %w", err)
	}

	// Read error info
	errorOffset, ok := l.module.mod.Memory().ReadUint32Le(uint32(errOffPtr))
	if !ok {
		return nil, errors.New("wasmts: read query error offset")
	}
	errorType, ok := l.module.mod.Memory().ReadUint32Le(uint32(errTypePtr))
	if !ok {
		return nil, errors.New("wasmts: read query error type")
	}

	if errorType != QueryErrorNone {
		return nil, formatQueryError(pattern, errorOffset, errorType)
	}

	queryPtr := result[0]
	if queryPtr == 0 {
		return nil, errors.New("wasmts: ts_query_new returned null")
	}

	return &Query{lang: l, ptr: queryPtr}, nil
}

// CaptureNameForID returns the name of a capture by its index.
func (q *Query) CaptureNameForID(id uint32) (string, error) {
	strlenPtr, err := q.lang.module.allocateBytes(4)
	if err != nil {
		return "", fmt.Errorf("wasmts: allocate strlen: %w", err)
	}
	defer q.lang.module.freePtr(strlenPtr)

	result, err := q.lang.module.call(_queryCaptureNameForID, q.ptr, uint64(id), strlenPtr)
	if err != nil {
		return "", fmt.Errorf("wasmts: ts_query_capture_name_for_id: %w", err)
	}

	nameLen, ok := q.lang.module.mod.Memory().ReadUint32Le(uint32(strlenPtr))
	if !ok {
		return "", errors.New("wasmts: read capture name length")
	}

	nameBytes, ok := q.lang.module.mod.Memory().Read(uint32(result[0]), nameLen)
	if !ok {
		return "", errors.New("wasmts: read capture name")
	}

	return string(nameBytes), nil
}

// Close releases the query from WASM memory.
// Since we now cache queries inside Language, we make Close a no-op
// so they persist for the lifetime of the Language instance.
func (q *Query) Close() {
	// No-op to allow caching
}

// --- QueryCursor ---

// NewQueryCursor creates a new query cursor for iterating matches.
func (l *Language) NewQueryCursor() (*QueryCursor, error) {
	result, err := l.module.call(_queryCursorNew)
	if err != nil {
		return nil, fmt.Errorf("wasmts: ts_query_cursor_new: %w", err)
	}
	return &QueryCursor{lang: l, ptr: result[0]}, nil
}

// Exec starts executing a query on the given node.
// Equivalent to smacker's qc.Exec(q, root).
func (qc *QueryCursor) Exec(q *Query, node *Node) error {
	_, err := qc.lang.module.call(_queryCursorExec, qc.ptr, q.ptr, node.ptr)
	return err
}

// NextMatch returns the next match, or (nil, false, nil) when done.
// Equivalent to smacker's match, ok := qc.NextMatch().
func (qc *QueryCursor) NextMatch(src []byte) (*QueryMatch, bool, error) {
	matchPtr, err := qc.lang.module.allocateBytes(tsQueryMatchSize)
	if err != nil {
		return nil, false, fmt.Errorf("wasmts: allocate query match: %w", err)
	}
	defer qc.lang.module.freePtr(matchPtr)

	result, err := qc.lang.module.call(_queryCursorNextMatch, qc.ptr, matchPtr)
	if err != nil {
		return nil, false, fmt.Errorf("wasmts: ts_query_cursor_next_match: %w", err)
	}
	if result[0] == 0 {
		return nil, false, nil // no more matches
	}

	// Read match struct fields
	mem := qc.lang.module.mod.Memory()

	matchID, ok := mem.ReadUint32Le(uint32(matchPtr))
	if !ok {
		return nil, false, errors.New("wasmts: read match ID")
	}
	patternIndex, ok := mem.ReadUint16Le(uint32(matchPtr) + 4)
	if !ok {
		return nil, false, errors.New("wasmts: read pattern index")
	}
	captureCount, ok := mem.ReadUint16Le(uint32(matchPtr) + 6)
	if !ok {
		return nil, false, errors.New("wasmts: read capture count")
	}
	capturesPtr, ok := mem.ReadUint32Le(uint32(matchPtr) + 8)
	if !ok {
		return nil, false, errors.New("wasmts: read captures pointer")
	}

	// Read captures
	captures := make([]QueryCapture, captureCount)
	addr := capturesPtr
	for i := range captureCount {
		// TSQueryCapture: TSNode (24 bytes) + uint32 index (4 bytes) = 28 bytes
		captureIndex, ok := mem.ReadUint32Le(addr + 24)
		if !ok {
			return nil, false, errors.New("wasmts: read capture index")
		}
		captures[i] = QueryCapture{
			ID:   captureIndex,
			Node: &Node{lang: qc.lang, ptr: uint64(addr), src: src},
		}
		addr += tsQueryCaptureSize
	}

	return &QueryMatch{
		ID:           matchID,
		PatternIndex: patternIndex,
		Captures:     captures,
	}, true, nil
}

// Close releases the query cursor from WASM memory.
func (qc *QueryCursor) Close() {
	if qc.ptr != 0 {
		qc.lang.module.call(_queryCursorDelete, qc.ptr) //nolint:errcheck
		qc.ptr = 0
	}
}

// --- Error formatting ---

var identifierRegexp = regexp.MustCompile(`^[a-zA-Z_][a-zA-Z0-9_-]*`)

func formatQueryError(pattern string, offset uint32, errorType uint32) error {
	line := 1
	lineStart := 0
	for i, c := range pattern {
		if uint32(i) >= offset {
			break
		}
		if c == '\n' {
			line++
			lineStart = i + 1
		}
	}
	column := int(offset) - lineStart
	typeName := queryErrorTypeName(errorType)

	var message string
	switch errorType {
	case QueryErrorNodeType, QueryErrorField, QueryErrorCapture:
		s := pattern[offset:]
		m := identifierRegexp.FindString(s)
		if m != "" {
			message = fmt.Sprintf("invalid %s '%s' at line %d column %d", typeName, m, line, column)
		} else {
			message = fmt.Sprintf("invalid %s at line %d column %d", typeName, line, column)
		}
	default:
		s := pattern[offset:]
		lines := strings.SplitN(s, "\n", 2)
		whitespace := strings.Repeat(" ", column)
		message = fmt.Sprintf("invalid %s at line %d column %d\n%s\n%s^", typeName, line, column, lines[0], whitespace)
	}

	return errors.New(message)
}

func queryErrorTypeName(t uint32) string {
	switch t {
	case QueryErrorNone:
		return "none"
	case QueryErrorSyntax:
		return "syntax"
	case QueryErrorNodeType:
		return "node type"
	case QueryErrorField:
		return "field"
	case QueryErrorCapture:
		return "capture"
	case QueryErrorStructure:
		return "structure"
	case QueryErrorLanguage:
		return "language"
	default:
		return "unknown"
	}
}
