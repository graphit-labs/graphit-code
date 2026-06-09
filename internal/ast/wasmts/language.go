package wasmts

import (
	"fmt"
)

// Language represents a loaded tree-sitter grammar.
// It wraps a WASM module and a pointer to the language obtained
// by calling tree_sitter_<name>().
type Language struct {
	name    string
	module  *Module
	ptr     uint64 // pointer returned by tree_sitter_<name>()
	queries map[string]*Query
}

// LoadLanguage loads a grammar from a WASM module.
// The WASM module must export a function named tree_sitter_<langName>()
// that returns a pointer to the TSLanguage struct.
//
// If the module contains a monolithic build (multiple grammars),
// langName selects which one to activate.
func (m *Module) LoadLanguage(langName string) (*Language, error) {
	fnName := "tree_sitter_" + langName

	fn := m.instance.GetFunc(m.store, fnName)
	if fn == nil {
		return nil, fmt.Errorf("wasmts: grammar function %q not found in module", fnName)
	}

	result, err := fn.Call(m.store)
	if err != nil {
		return nil, fmt.Errorf("wasmts: call %s: %w", fnName, err)
	}

	var langPtr uint64
	switch v := result.(type) {
	case int32:
		langPtr = uint64(v)
	case int64:
		langPtr = uint64(v)
	case uint32:
		langPtr = uint64(v)
	case uint64:
		langPtr = v
	default:
		return nil, fmt.Errorf("wasmts: unexpected return type %T from %s", result, fnName)
	}

	if langPtr == 0 {
		return nil, fmt.Errorf("wasmts: %s returned null pointer", fnName)
	}

	return &Language{
		name:    langName,
		module:  m,
		ptr:     langPtr,
		queries: make(map[string]*Query),
	}, nil
}

func (l *Language) Name() string {
	return l.name
}

func (l *Language) Module() *Module {
	return l.module
}

func (l *Language) Version() (uint64, error) {
	result, err := l.module.call(_languageVersion, l.ptr)
	if err != nil {
		return 0, fmt.Errorf("wasmts: language version: %w", err)
	}
	return result[0], nil
}

// ListAvailableLanguages introspects a module for all tree_sitter_* exports.
// Useful for discovering languages in a monolithic WASM build.
func (m *Module) ListAvailableLanguages() []string {
	var langs []string
	for _, exp := range m.wasmMod.Exports() {
		name := exp.Name()
		if len(name) > 12 && name[:12] == "tree_sitter_" {
			langs = append(langs, name[12:])
		}
	}
	return langs
}
