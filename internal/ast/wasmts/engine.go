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

type compiledEntry struct {
	compiled wazero.CompiledModule
}

// Engine manages the wazero runtime and loaded WASM modules.
type Engine struct {
	ctx      context.Context
	rt       wazero.Runtime
	cache    wazero.CompilationCache
	mu       sync.Mutex
	compiled map[string]*compiledEntry
	modules  map[string]*Module
	nextID   atomic.Int64
	closed   bool
}

// Module represents a loaded WASM module instance.
// NOT thread-safe — each goroutine must use its own Module.
type Module struct {
	mod  api.Module
	fns  map[string]api.Function
	ctx  context.Context
}

// NewEngine creates a wazero runtime with compilation cache and WASI.
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


	var compilationCache wazero.CompilationCache
	if cacheDir != "" {
		var err error
		compilationCache, err = wazero.NewCompilationCacheWithDir(cacheDir)
		if err == nil {
			cfg = cfg.WithCompilationCache(compilationCache)
		}
	}

	rt := wazero.NewRuntimeWithConfig(ctx, cfg)


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

// LoadModule compiles and instantiates a WASM binary. Thread-safe.
// Retains the compiled form for InstantiateModule.
func (e *Engine) LoadModule(name string, wasmBytes []byte) (*Module, error) {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return nil, fmt.Errorf("wasmts: engine closed")
	}


	if mod, ok := e.modules[name]; ok {
		return mod, nil
	}


	compiled, err := e.rt.CompileModule(e.ctx, wasmBytes)
	if err != nil {
		return nil, fmt.Errorf("wasmts: compile module %q: %w", name, err)
	}


	e.compiled[name] = &compiledEntry{compiled: compiled}


	mod, err := e.instantiate(name, compiled)
	if err != nil {
		return nil, err
	}

	e.modules[name] = mod
	return mod, nil
}

// InstantiateModule creates a new isolated instance from a compiled module.
// Each instance has its own linear memory. Thread-safe.
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

func (e *Engine) instantiate(name string, compiled wazero.CompiledModule) (*Module, error) {
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


func (e *Engine) HasCompiledModule(name string) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	_, ok := e.compiled[name]
	return ok
}


func (e *Engine) GetModule(name string) *Module {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.modules[name]
}


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


func (m *Module) call(name string, args ...uint64) ([]uint64, error) {
	fn, ok := m.fns[name]
	if !ok {
		return nil, fmt.Errorf("wasmts: function %q not exported", name)
	}
	return fn.Call(m.ctx, args...)
}


func (m *Module) CloseModule() error {
	return m.mod.Close(m.ctx)
}


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

	m.mod.Memory().WriteByte(uint32(p)+uint32(sz), 0)

	return p, sz, func() {
		m.call(_free, p) //nolint:errcheck
	}, nil
}


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


func (m *Module) allocateBytes(n uint64) (uint64, error) {
	result, err := m.call(_malloc, n)
	if err != nil {
		return 0, fmt.Errorf("wasmts: malloc(%d): %w", n, err)
	}
	return result[0], nil
}


func (m *Module) freePtr(ptr uint64) {
	m.call(_free, ptr) //nolint:errcheck
}


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
