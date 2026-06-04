package ast

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
)

// WorkerModules holds per-worker WASM module instances.
//
// Each pipeline worker goroutine gets its own WorkerModules. Module instances
// are created lazily — only when the worker encounters a file of that language
// for the first time. Each instance has its own WASM linear memory, so no
// mutexes or synchronization is needed.
//
// This is the key to restoring full parallelism: N workers can parse N files
// of the same language simultaneously, each with their own isolated memory.
type WorkerModules struct {
	engine    *wasmts.Engine
	modules   map[string]*wasmts.Module   // module name → worker-local instance
	languages map[string]*wasmts.Language  // lang func name → worker-local Language
}

// NewWorkerModules creates a new per-worker module set.
// The engine must be the global shared engine (from GetEngine()).
func NewWorkerModules(engine *wasmts.Engine) *WorkerModules {
	return &WorkerModules{
		engine:    engine,
		modules:   make(map[string]*wasmts.Module),
		languages: make(map[string]*wasmts.Language),
	}
}

// GetLanguage returns a worker-local Language instance for the given config.
// On first call for a given language, it instantiates a new WASM module from
// the pre-compiled bytecode (cheap — AOT compilation is cached, only memory
// allocation happens here).
//
// The funcName is the tree-sitter grammar function name (e.g., "go", "c_sharp").
func (wm *WorkerModules) GetLanguage(funcName string) (*wasmts.Language, error) {
	if wm == nil || wm.engine == nil {
		return nil, fmt.Errorf("worker modules not initialized")
	}

	// Return cached worker-local instance
	if lang, ok := wm.languages[funcName]; ok {
		return lang, nil
	}

	// Instantiate a new isolated module for this worker
	moduleName := "tree-sitter-" + funcName
	mod, err := wm.engine.InstantiateModule(moduleName)
	if err != nil {
		return nil, fmt.Errorf("worker instantiate %q: %w", moduleName, err)
	}

	// Load the language from the new module instance
	lang, err := mod.LoadLanguage(funcName)
	if err != nil {
		mod.CloseModule() //nolint:errcheck
		return nil, fmt.Errorf("worker load language %q: %w", funcName, err)
	}

	wm.modules[moduleName] = mod
	wm.languages[funcName] = lang
	return lang, nil
}

// Close releases all worker-local WASM module instances.
// Must be called when the worker goroutine finishes.
func (wm *WorkerModules) Close() {
	if wm == nil {
		return
	}
	for _, mod := range wm.modules {
		mod.CloseModule() //nolint:errcheck
	}
	wm.modules = nil
	wm.languages = nil
}
