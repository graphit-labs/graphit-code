package wasmantlr

import (
	"context"
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"sync"

	"github.com/bytecodealliance/wasmtime-go/v21"
)

// WasmMemory is a compatibility wrapper to read/write linear WASM memory.
type WasmMemory struct {
	mem   *wasmtime.Memory
	store wasmtime.Storelike
}

func (m *WasmMemory) ReadUint32Be(offset uint32) (uint32, bool) {
	data := m.mem.UnsafeData(m.store)
	if int(offset)+4 > len(data) {
		return 0, false
	}
	return binary.BigEndian.Uint32(data[offset : offset+4]), true
}

func (m *WasmMemory) Read(offset uint32, byteCount uint32) ([]byte, bool) {
	data := m.mem.UnsafeData(m.store)
	if int(offset)+int(byteCount) > len(data) {
		return nil, false
	}
	res := make([]byte, byteCount)
	copy(res, data[offset:offset+byteCount])
	return res, true
}

func (m *WasmMemory) Write(offset uint32, b []byte) bool {
	data := m.mem.UnsafeData(m.store)
	if int(offset)+len(b) > len(data) {
		return false
	}
	copy(data[offset:offset+uint32(len(b))], b)
	return true
}

// Engine manages compiling and caching of ANTLR WASM modules.
type Engine struct {
	ctx      context.Context
	engine   *wasmtime.Engine
	linker   *wasmtime.Linker
	mu       sync.Mutex
	compiled map[string]*wasmtime.Module
	closed   bool
}

// Module represents an instantiated ANTLR WASM module on a single thread.
type Module struct {
	store    *wasmtime.Store
	instance *wasmtime.Instance
	mem      *WasmMemory
	malloc   *wasmtime.Func
	free     *wasmtime.Func
	parse    *wasmtime.Func
}

// NewEngine creates a new WASM engine for ANTLR with JIT caching and WASI enabled.
func NewEngine(cacheDir string) (*Engine, error) {
	ctx := context.Background()
	config := wasmtime.NewConfig()

	if cacheDir != "" {
		cachePath := filepath.Join(cacheDir, "wasmtime_antlr_cache_config.toml")
		cacheDataPath := filepath.Join(cacheDir, "wasmtime_antlr_cache")
		_ = os.MkdirAll(cacheDataPath, 0755)

		tomlContent := fmt.Sprintf(`[cache]
enabled = true
directory = %q
`, cacheDataPath)
		if err := os.WriteFile(cachePath, []byte(tomlContent), 0644); err == nil {
			_ = config.CacheConfigLoad(cachePath)
		}
	} else {
		_ = config.CacheConfigLoadDefault()
	}

	engine := wasmtime.NewEngineWithConfig(config)
	linker := wasmtime.NewLinker(engine)

	err := linker.DefineWasi()
	if err != nil {
		engine.Close()
		return nil, fmt.Errorf("wasmantlr: define WASI: %w", err)
	}

	return &Engine{
		ctx:      ctx,
		engine:   engine,
		linker:   linker,
		compiled: make(map[string]*wasmtime.Module),
	}, nil
}

// LoadModule compiles the WASM binary. Thread-safe.
func (e *Engine) LoadModule(name string, wasmBytes []byte) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return fmt.Errorf("wasmantlr: engine closed")
	}

	if _, ok := e.compiled[name]; ok {
		return nil
	}

	module, err := wasmtime.NewModule(e.engine, wasmBytes)
	if err != nil {
		return fmt.Errorf("wasmantlr: compile module %q: %w", name, err)
	}

	e.compiled[name] = module
	return nil
}

// HasCompiledModule reports whether a grammar has been compiled. Thread-safe.
func (e *Engine) HasCompiledModule(name string) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	_, ok := e.compiled[name]
	return ok
}

// InstantiateModule creates an isolated WASI/WASM instance from a compiled module.
func (e *Engine) InstantiateModule(name string) (*Module, error) {
	e.mu.Lock()
	module, ok := e.compiled[name]
	closed := e.closed
	e.mu.Unlock()

	if closed {
		return nil, fmt.Errorf("wasmantlr: engine closed")
	}
	if !ok {
		return nil, fmt.Errorf("wasmantlr: module %q not compiled (call LoadModule first)", name)
	}

	store := wasmtime.NewStore(e.engine)
	wasiConfig := wasmtime.NewWasiConfig()
	store.SetWasi(wasiConfig)

	instance, err := e.linker.Instantiate(store, module)
	if err != nil {
		return nil, fmt.Errorf("wasmantlr: instantiate module %q: %w", name, err)
	}

	// Initialize Go WASM reactor runtime
	initFn := instance.GetFunc(store, "_initialize")
	if initFn != nil {
		_, err = initFn.Call(store)
		if err != nil {
			return nil, fmt.Errorf("wasmantlr: initialize runtime %q: %w", name, err)
		}
	}

	memExport := instance.GetExport(store, "memory")
	if memExport == nil {
		return nil, fmt.Errorf("wasmantlr: memory export not found in %q", name)
	}
	mem := memExport.Memory()

	mallocFn := instance.GetFunc(store, "malloc")
	freeFn := instance.GetFunc(store, "free")
	parseFn := instance.GetFunc(store, "parse_antlr")

	if mallocFn == nil || freeFn == nil || parseFn == nil {
		return nil, fmt.Errorf("wasmantlr: missing required functions (malloc, free, parse_antlr) in %q", name)
	}

	return &Module{
		store:    store,
		instance: instance,
		mem:      &WasmMemory{mem: mem, store: store},
		malloc:   mallocFn,
		free:     freeFn,
		parse:    parseFn,
	}, nil
}

// Parse invokes the WASM-exported parser on the given source bytes and returns the JSON tree bytes.
func (m *Module) Parse(source []byte) ([]byte, error) {
	srcLen := len(source)
	if srcLen == 0 {
		return []byte(`{"type":"error","message":"empty_source"}`), nil
	}

	// 1. Allocate buffer in WASM memory for the source code
	mallocRes, err := m.malloc.Call(m.store, int32(srcLen))
	if err != nil {
		return nil, fmt.Errorf("wasmantlr: call malloc: %w", err)
	}
	srcPtr := uint32(mallocRes.(int32))
	defer func() {
		_, _ = m.free.Call(m.store, int32(srcPtr))
	}()

	// 2. Write source bytes into the buffer
	if !m.mem.Write(srcPtr, source) {
		return nil, fmt.Errorf("wasmantlr: failed to write source to memory")
	}

	// 3. Invoke parse_antlr
	parseRes, err := m.parse.Call(m.store, int32(srcPtr), int32(srcLen))
	if err != nil {
		return nil, fmt.Errorf("wasmantlr: call parse_antlr: %w", err)
	}
	respPtr := uint32(parseRes.(int32))
	if respPtr == 0 {
		return nil, fmt.Errorf("wasmantlr: parse returned null pointer")
	}
	defer func() {
		_, _ = m.free.Call(m.store, int32(respPtr))
	}()

	// 4. Read response length: [4 bytes BigEndian length]
	lenBytes, ok := m.mem.Read(respPtr, 4)
	if !ok {
		return nil, fmt.Errorf("wasmantlr: failed to read response length")
	}
	respLen := binary.BigEndian.Uint32(lenBytes)

	// 5. Read response JSON payload
	respBytes, ok := m.mem.Read(respPtr+4, respLen)
	if !ok {
		return nil, fmt.Errorf("wasmantlr: failed to read response payload of length %d", respLen)
	}

	// Make a copy of the slice to detach it from the temporary WASM memory pointer
	res := make([]byte, len(respBytes))
	copy(res, respBytes)

	return res, nil
}

func (e *Engine) Close() error {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.closed = true
	e.engine.Close()
	return nil
}
