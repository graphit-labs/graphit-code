package wasmts

import (
	"fmt"
)

// Parser wraps a tree-sitter parser instance in WASM memory.
type Parser struct {
	lang *Language
	ptr  uint64
}

// NewParser creates a new tree-sitter parser and sets its language.
func (l *Language) NewParser() (*Parser, error) {
	result, err := l.module.call(_parserNew)
	if err != nil {
		return nil, fmt.Errorf("wasmts: ts_parser_new: %w", err)
	}
	parserPtr := result[0]
	if parserPtr == 0 {
		return nil, fmt.Errorf("wasmts: ts_parser_new returned null")
	}

	// Set language
	ok, err := l.module.call(_parserSetLanguage, parserPtr, l.ptr)
	if err != nil {
		l.module.call(_parserDelete, parserPtr) //nolint:errcheck
		return nil, fmt.Errorf("wasmts: ts_parser_set_language: %w", err)
	}
	if ok[0] == 0 {
		l.module.call(_parserDelete, parserPtr) //nolint:errcheck
		return nil, fmt.Errorf("wasmts: incompatible language version")
	}

	return &Parser{
		lang: l,
		ptr:  parserPtr,
	}, nil
}

// Parse parses source code and returns a Tree.
// The source bytes are retained in the Tree for Content() lookups.
func (p *Parser) Parse(source []byte) (*Tree, error) {
	// Allocate source in WASM memory
	srcSize := uint64(len(source))
	srcPtr, err := p.lang.module.allocateBytes(srcSize + 1) // +1 for potential null
	if err != nil {
		return nil, fmt.Errorf("wasmts: allocate source: %w", err)
	}

	if !p.lang.module.mod.Memory().Write(uint32(srcPtr), source) {
		p.lang.module.freePtr(srcPtr)
		return nil, fmt.Errorf("wasmts: write source to memory")
	}

	// ts_parser_parse_string(parser, old_tree, string, length)
	result, err := p.lang.module.call(_parserParseString, p.ptr, 0, srcPtr, srcSize)
	p.lang.module.freePtr(srcPtr) // free source from WASM memory
	if err != nil {
		return nil, fmt.Errorf("wasmts: ts_parser_parse_string: %w", err)
	}

	treePtr := result[0]
	if treePtr == 0 {
		return nil, fmt.Errorf("wasmts: parse returned null tree")
	}

	return &Tree{
		lang: p.lang,
		ptr:  treePtr,
		src:  source,
	}, nil
}

// Close releases the parser from WASM memory.
func (p *Parser) Close() {
	if p.ptr != 0 {
		p.lang.module.call(_parserDelete, p.ptr) //nolint:errcheck
		p.ptr = 0
	}
}
