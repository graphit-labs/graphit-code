package wasmts

import (
	"context"
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"sync"

	"github.com/bytecodealliance/wasmtime-go/v21"
)

// WasmMemory is a compatibility wrapper implementing the wazero memory API
// on top of Wasmtime's raw linear memory slice.
type WasmMemory struct {
	mem   *wasmtime.Memory
	store wasmtime.Storelike
}

func (m *WasmMemory) ReadUint32Le(offset uint32) (uint32, bool) {
	data := m.mem.UnsafeData(m.store)
	if int(offset)+4 > len(data) {
		return 0, false
	}
	return binary.LittleEndian.Uint32(data[offset : offset+4]), true
}

func (m *WasmMemory) ReadUint16Le(offset uint32) (uint16, bool) {
	data := m.mem.UnsafeData(m.store)
	if int(offset)+2 > len(data) {
		return 0, false
	}
	return binary.LittleEndian.Uint16(data[offset : offset+2]), true
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

func (m *WasmMemory) WriteByteAt(offset uint32, v byte) bool {
	data := m.mem.UnsafeData(m.store)
	if int(offset)+1 > len(data) {
		return false
	}
	data[offset] = v
	return true
}

// ModuleMemoryWrapper exposes the Memory() method matching the old API
// to avoid breaking changes in parser.go, query.go, tree.go, and node.go.
type ModuleMemoryWrapper struct {
	mem *WasmMemory
}

func (w *ModuleMemoryWrapper) Memory() *WasmMemory {
	return w.mem
}

// Engine manages the Wasmtime runtime and loaded WASM modules.
type Engine struct {
	ctx      context.Context
	engine   *wasmtime.Engine
	linker   *wasmtime.Linker
	mu       sync.Mutex
	compiled map[string]*wasmtime.Module
	modules  map[string]*Module
	closed   bool
	cacheDir string
}

// Module represents a loaded WASM module instance.
// NOT thread-safe — each goroutine must use its own Module.
type Module struct {
	store    *wasmtime.Store
	instance *wasmtime.Instance
	mod      *ModuleMemoryWrapper
	fns      map[string]*wasmtime.Func
	ctx      context.Context
	wasmMod  *wasmtime.Module
}

// NewEngine creates a Wasmtime engine with JIT compilation caching and WASI.
func NewEngine(cacheDir string) (*Engine, error) {
	ctx := context.Background()

	config := wasmtime.NewConfig()

	if cacheDir != "" {
		cachePath := filepath.Join(cacheDir, "wasmtime_cache_config.toml")
		cacheDataPath := filepath.Join(cacheDir, "wasmtime_cache")
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
		return nil, fmt.Errorf("wasmts: define WASI: %w", err)
	}

	return &Engine{
		ctx:      ctx,
		engine:   engine,
		linker:   linker,
		compiled: make(map[string]*wasmtime.Module),
		modules:  make(map[string]*Module),
		cacheDir: cacheDir,
	}, nil
}

// LoadModule compiles and instantiates a WASM binary. Thread-safe.
func (e *Engine) LoadModule(name string, wasmBytes []byte) (*Module, error) {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return nil, fmt.Errorf("wasmts: engine closed")
	}

	if mod, ok := e.modules[name]; ok {
		return mod, nil
	}

	module, err := wasmtime.NewModule(e.engine, wasmBytes)
	if err != nil {
		return nil, fmt.Errorf("wasmts: compile module %q: %w", name, err)
	}

	e.compiled[name] = module

	mod, err := e.instantiate(name, module)
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
	module, ok := e.compiled[name]
	closed := e.closed
	e.mu.Unlock()

	if closed {
		return nil, fmt.Errorf("wasmts: engine closed")
	}
	if !ok {
		return nil, fmt.Errorf("wasmts: module %q not compiled (call LoadModule first)", name)
	}

	return e.instantiate(name, module)
}

func (e *Engine) instantiate(name string, module *wasmtime.Module) (*Module, error) {
	store := wasmtime.NewStore(e.engine)
	wasiConfig := wasmtime.NewWasiConfig()
	store.SetWasi(wasiConfig)

	instance, err := e.linker.Instantiate(store, module)
	if err != nil {
		return nil, fmt.Errorf("wasmts: instantiate module %q: %w", name, err)
	}

	memExport := instance.GetExport(store, "memory")
	if memExport == nil {
		return nil, fmt.Errorf("wasmts: memory export not found in %q", name)
	}
	mem := memExport.Memory()
	wasmMem := &WasmMemory{mem: mem, store: store}
	memWrapper := &ModuleMemoryWrapper{mem: wasmMem}

	fns := make(map[string]*wasmtime.Func, len(_coreFunctions))
	for _, fnName := range _coreFunctions {
		fn := instance.GetFunc(store, fnName)
		if fn != nil {
			fns[fnName] = fn
		}
	}

	return &Module{
		store:    store,
		instance: instance,
		mod:      memWrapper,
		fns:      fns,
		ctx:      e.ctx,
		wasmMod:  module,
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
	e.engine.Close()
	return nil
}

func (m *Module) call(name string, args ...uint64) ([]uint64, error) {
	fn, ok := m.fns[name]
	if !ok {
		return nil, fmt.Errorf("wasmts: function %q not exported", name)
	}

	ft := fn.Type(m.store)
	params := ft.Params()
	if len(args) != len(params) {
		return nil, fmt.Errorf("wasmts: function %q called with %d args, expected %d", name, len(args), len(params))
	}

	converted := make([]interface{}, len(args))
	for i, arg := range args {
		switch params[i].Kind() {
		case wasmtime.KindI32:
			converted[i] = int32(arg)
		case wasmtime.KindI64:
			converted[i] = int64(arg)
		default:
			converted[i] = arg
		}
	}

	res, err := fn.Call(m.store, converted...)
	if err != nil {
		return nil, fmt.Errorf("wasmts: call %s: %w", name, err)
	}

	results := ft.Results()
	if len(results) == 0 {
		return nil, nil
	}

	if len(results) == 1 {
		var val uint64
		switch v := res.(type) {
		case int32:
			val = uint64(v)
		case uint32:
			val = uint64(v)
		case int64:
			val = uint64(v)
		case uint64:
			val = v
		default:
			return nil, fmt.Errorf("wasmts: unexpected return type %T from %s", res, name)
		}
		return []uint64{val}, nil
	}

	if slice, ok := res.([]interface{}); ok {
		ret := make([]uint64, len(slice))
		for i, item := range slice {
			switch v := item.(type) {
			case int32:
				ret[i] = uint64(v)
			case uint32:
				ret[i] = uint64(v)
			case int64:
				ret[i] = uint64(v)
			case uint64:
				ret[i] = uint64(v)
			default:
				return nil, fmt.Errorf("wasmts: unexpected return type %T from %s", item, name)
			}
		}
		return ret, nil
	}

	return nil, fmt.Errorf("wasmts: unexpected return type %T from %s", res, name)
}

func (m *Module) CloseModule() error {
	return nil
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

	m.mod.Memory().WriteByteAt(uint32(p)+uint32(sz), 0)

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
