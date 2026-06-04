package wasmts

import (
	"context"
	"crypto/rand"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

// compiledEntry holds a compiled (but not yet instantiated) WASM module.
// The compiled form is reusable: each call to InstantiateModule creates
// a fresh instance with its own isolated linear memory.
type compiledEntry struct {
	compiled wazero.CompiledModule
}

// Engine manages the wazero runtime and loaded WASM modules.
// A single Engine instance is shared across all goroutines.
// Each loaded .wasm grammar gets its own compiled+instantiated module.
type Engine struct {
	ctx      context.Context
	rt       wazero.Runtime
	cache    wazero.CompilationCache
	mu       sync.Mutex
	compiled map[string]*compiledEntry // module name → compiled (for re-instantiation)
	modules  map[string]*Module        // module name → singleton instance (backward compat)
	nextID   atomic.Int64              // unique suffix for worker module instance names
	closed   bool
}

// Module represents a loaded and instantiated WASM module.
// It holds cached references to all exported functions.
//
// Each Module wraps one WASM linear memory region. WASM memory is NOT
// thread-safe — a Module must only be used by a single goroutine at a time.
// The per-worker architecture (WorkerModules) guarantees this by giving
// each worker goroutine its own Module instances.
type Module struct {
	mod  api.Module
	fns  map[string]api.Function
	ctx  context.Context
}

// NewEngine creates a wazero runtime with AOT compilation where supported,
// disk-based compilation cache, and WASI support.
func NewEngine(cacheDir string) (*Engine, error) {
	ctx := context.Background()

	var cfg wazero.RuntimeConfig
	if compilerSupported() {
		cfg = wazero.NewRuntimeConfigCompiler()
	} else {
		cfg = wazero.NewRuntimeConfigInterpreter()
	}
	cfg = cfg.WithMemoryLimitPages(1024) // 64MB max
	cfg = cfg.WithCoreFeatures(api.CoreFeaturesV2)

	// Enable disk-based compilation cache for faster subsequent loads
	var compilationCache wazero.CompilationCache
	if cacheDir != "" {
		var err error
		compilationCache, err = wazero.NewCompilationCacheWithDir(cacheDir)
		if err == nil {
			cfg = cfg.WithCompilationCache(compilationCache)
		}
	}

	rt := wazero.NewRuntimeWithConfig(ctx, cfg)

	// WASI is required for the tree-sitter WASM modules
	_, err := wasi_snapshot_preview1.Instantiate(ctx, rt)
	if err != nil {
		rt.Close(ctx)
		return nil, fmt.Errorf("wasmts: instantiate WASI: %w", err)
	}

	return &Engine{
		ctx:      ctx,
		rt:       rt,
		cache:    compilationCache,
		compiled: make(map[string]*compiledEntry),
		modules:  make(map[string]*Module),
	}, nil
}

// LoadModule compiles and instantiates a WASM binary, returning a Module.
// The module name is used for caching — loading the same name twice returns
// the cached module. Thread-safe.
//
// The compiled form is retained so that InstantiateModule can create
// additional isolated instances for per-worker use.
func (e *Engine) LoadModule(name string, wasmBytes []byte) (*Module, error) {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return nil, fmt.Errorf("wasmts: engine closed")
	}

	// Return cached if already loaded
	if mod, ok := e.modules[name]; ok {
		return mod, nil
	}

	// Compile (AOT compilation result is cached on disk)
	compiled, err := e.rt.CompileModule(e.ctx, wasmBytes)
	if err != nil {
		return nil, fmt.Errorf("wasmts: compile module %q: %w", name, err)
	}

	// Store compiled form for future InstantiateModule calls
	e.compiled[name] = &compiledEntry{compiled: compiled}

	// Instantiate the singleton (global) instance
	mod, err := e.instantiate(name, compiled)
	if err != nil {
		return nil, err
	}

	e.modules[name] = mod
	return mod, nil
}

// InstantiateModule creates a new isolated instance of a previously compiled
// WASM module. Each instance has its own linear memory, so it can be used
// concurrently with other instances of the same module without any locking.
//
// This is the key method for per-worker parallelism: each worker goroutine
// calls InstantiateModule to get its own Module, avoiding all contention.
// Thread-safe (uses atomic counter for unique names).
func (e *Engine) InstantiateModule(name string) (*Module, error) {
	e.mu.Lock()
	entry, ok := e.compiled[name]
	closed := e.closed
	e.mu.Unlock()

	if closed {
		return nil, fmt.Errorf("wasmts: engine closed")
	}
	if !ok {
		return nil, fmt.Errorf("wasmts: module %q not compiled (call LoadModule first)", name)
	}

	return e.instantiate(name, entry.compiled)
}

// instantiate creates a new module instance from a compiled module.
// Each call produces a unique instance name to avoid wazero conflicts.
func (e *Engine) instantiate(name string, compiled wazero.CompiledModule) (*Module, error) {
	// Generate unique instance name (wazero requires unique names per runtime)
	id := e.nextID.Add(1)
	instanceName := fmt.Sprintf("%s-i%d", name, id)

	modCfg := wazero.NewModuleConfig().
		WithName(instanceName).
		WithSysNanosleep().
		WithSysNanotime().
		WithSysWalltime().
		WithRandSource(rand.Reader)

	inst, err := e.rt.InstantiateModule(e.ctx, compiled, modCfg)
	if err != nil {
		return nil, fmt.Errorf("wasmts: instantiate module %q: %w", name, err)
	}

	// Cache exported functions
	fns := make(map[string]api.Function, len(_coreFunctions))
	for _, fnName := range _coreFunctions {
		fn := inst.ExportedFunction(fnName)
		if fn != nil {
			fns[fnName] = fn
		}
	}

	return &Module{
		mod: inst,
		fns: fns,
		ctx: e.ctx,
	}, nil
}

// HasCompiledModule returns true if the named module has been compiled.
func (e *Engine) HasCompiledModule(name string) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	_, ok := e.compiled[name]
	return ok
}

// GetModule returns a previously loaded module by name, or nil.
func (e *Engine) GetModule(name string) *Module {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.modules[name]
}

// Close shuts down the engine and all loaded modules.
func (e *Engine) Close() error {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.closed = true
	err := e.rt.Close(e.ctx)
	if e.cache != nil {
		_ = e.cache.Close(e.ctx)
	}
	return err
}

// --- Module helpers ---

// call invokes an exported function by name with the given arguments.
func (m *Module) call(name string, args ...uint64) ([]uint64, error) {
	fn, ok := m.fns[name]
	if !ok {
		return nil, fmt.Errorf("wasmts: function %q not exported", name)
	}
	return fn.Call(m.ctx, args...)
}

// CloseModule closes a single module instance, releasing its WASM memory.
// Used by WorkerModules to clean up per-worker instances.
func (m *Module) CloseModule() error {
	return m.mod.Close(m.ctx)
}

// allocateString writes a Go string into WASM linear memory.
// Returns (pointer, size, free_func, error). Caller must call free_func().
func (m *Module) allocateString(s string) (ptr, size uint64, free func(), err error) {
	b := []byte(s)
	sz := uint64(len(b))

	result, err := m.call(_malloc, sz+1) // +1 for null terminator
	if err != nil {
		return 0, 0, nil, fmt.Errorf("wasmts: malloc for string: %w", err)
	}
	p := result[0]

	if !m.mod.Memory().Write(uint32(p), b) {
		return 0, 0, nil, fmt.Errorf("wasmts: write string to memory")
	}
	// Write null terminator
	m.mod.Memory().WriteByte(uint32(p)+uint32(sz), 0)

	return p, sz, func() {
		m.call(_free, p) //nolint:errcheck
	}, nil
}

// readString reads a null-terminated C string from WASM memory.
func (m *Module) readString(ptr uint64) (string, error) {
	result, err := m.call(_strlen, ptr)
	if err != nil {
		return "", fmt.Errorf("wasmts: strlen: %w", err)
	}
	length := uint32(result[0])
	bytes, ok := m.mod.Memory().Read(uint32(ptr), length)
	if !ok {
		return "", fmt.Errorf("wasmts: read string from memory")
	}
	return string(bytes), nil
}

// allocateBytes allocates n bytes in WASM memory and returns the pointer.
func (m *Module) allocateBytes(n uint64) (uint64, error) {
	result, err := m.call(_malloc, n)
	if err != nil {
		return 0, fmt.Errorf("wasmts: malloc(%d): %w", n, err)
	}
	return result[0], nil
}

// freePtr releases memory at the given pointer.
func (m *Module) freePtr(ptr uint64) {
	m.call(_free, ptr) //nolint:errcheck
}

// compilerSupported returns true if wazero's AOT compiler is available.
func compilerSupported() bool {
	switch runtime.GOOS {
	case "linux", "android", "windows", "darwin",
		"freebsd", "netbsd", "dragonfly", "solaris", "illumos":
	default:
		return false
	}
	switch runtime.GOARCH {
	case "amd64", "arm64":
		return true
	default:
		return false
	}
}
