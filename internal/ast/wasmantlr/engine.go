package wasmantlr

import (
	"context"
	"crypto/rand"
	"encoding/binary"
	"fmt"
	"io"
	"os/exec"
	"sync"
	"sync/atomic"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

var workerCounter atomic.Int64

// Engine manages ANTLR parser backends using length-prefixed IPC protocol.
//
// Both native subprocesses and WASM instances use the same protocol:
// request  = [4 bytes BE length][source bytes]
// response = [4 bytes BE length][JSON parse tree]
//
// The parser binary loops reading requests until stdin closes. ATN tables
// are initialized once on first parse and reused for all subsequent calls.
type Engine struct {
	ctx      context.Context
	rt       wazero.Runtime
	cache    wazero.CompilationCache
	mu       sync.Mutex
	procs    map[string]*ParserProc // native or WASM — same protocol
	compiled map[string]wazero.CompiledModule
	closed   bool
}

// ParserProc is a persistent parser process (native binary or WASM instance).
// Communication uses length-prefixed messages (4-byte big-endian length + payload).
type ParserProc struct {
	Stdin  io.Writer
	Stdout io.Reader
	Close  func()
	Wait   func() // blocks until the background goroutine exits
	Mu     sync.Mutex
}

// NewEngine creates a runtime for ANTLR parsers.
func NewEngine(cacheDir string) (*Engine, error) {
	ctx := context.Background()

	cfg := wazero.NewRuntimeConfigCompiler()
	cfg = cfg.WithMemoryLimitPages(16384) // 1GB — Go WASM runtime needs generous memory
	cfg = cfg.WithCoreFeatures(api.CoreFeaturesV2)

	var compilationCache wazero.CompilationCache
	if cacheDir != "" {
		var err error
		compilationCache, err = wazero.NewCompilationCacheWithDir(cacheDir)
		if err != nil {
			return nil, fmt.Errorf("wasmantlr: create compilation cache: %w", err)
		}
		cfg = cfg.WithCompilationCache(compilationCache)
	}

	rt := wazero.NewRuntimeWithConfig(ctx, cfg)
	_, err := wasi_snapshot_preview1.Instantiate(ctx, rt)
	if err != nil {
		rt.Close(ctx)
		return nil, fmt.Errorf("wasmantlr: instantiate WASI: %w", err)
	}

	return &Engine{
		ctx:      ctx,
		rt:       rt,
		cache:    compilationCache,
		procs:    make(map[string]*ParserProc),
		compiled: make(map[string]wazero.CompiledModule),
	}, nil
}

// Compile compiles a WASM binary and starts a persistent instance.
func (e *Engine) Compile(name string, wasmBytes []byte) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return fmt.Errorf("wasmantlr: engine closed")
	}
	if _, ok := e.procs[name]; ok {
		return nil
	}

	compiled, err := e.rt.CompileModule(e.ctx, wasmBytes)
	if err != nil {
		return fmt.Errorf("wasmantlr: compile module %q: %w", name, err)
	}

	e.compiled[name] = compiled

	proc, err := e.startWASMProc(name, compiled)
	if err != nil {
		return fmt.Errorf("wasmantlr: start WASM instance %q: %w", name, err)
	}

	e.procs[name] = proc
	return nil
}

// RegisterNativeBinary registers a native parser binary for a grammar.
func (e *Engine) RegisterNativeBinary(name, binaryPath string) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if e.closed {
		return fmt.Errorf("wasmantlr: engine closed")
	}

	proc, err := startNativeProc(binaryPath)
	if err != nil {
		return fmt.Errorf("wasmantlr: start native parser %q: %w", name, err)
	}
	e.procs[name] = proc
	return nil
}

// HasCompiled reports whether a parser is registered.
func (e *Engine) HasCompiled(name string) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	_, ok := e.procs[name]
	if !ok {
		_, ok = e.compiled[name]
	}
	return ok
}

// NewWorkerProc creates a per-worker WASM instance from the shared compiled module.
// NOT thread-safe — one proc per goroutine.
func (e *Engine) NewWorkerProc(name string) (*ParserProc, error) {
	e.mu.Lock()
	compiled, ok := e.compiled[name]
	closed := e.closed
	e.mu.Unlock()

	if closed {
		return nil, fmt.Errorf("wasmantlr: engine closed")
	}
	if !ok {
		return nil, fmt.Errorf("wasmantlr: module %q not compiled", name)
	}

	return e.startWASMProc(fmt.Sprintf("%s-w%d", name, workerCounter.Add(1)), compiled)
}

// Parse sends source to the named parser and returns the parsed tree.
func (e *Engine) Parse(name string, source []byte) (*TreeNode, error) {
	e.mu.Lock()
	proc := e.procs[name]
	closed := e.closed
	e.mu.Unlock()

	if closed {
		return nil, fmt.Errorf("wasmantlr: engine closed")
	}

	if proc == nil {
		return nil, fmt.Errorf("wasmantlr: module %q not compiled (call Compile or RegisterNativeBinary first)", name)
	}

	proc.Mu.Lock()
	defer proc.Mu.Unlock()


	var lenBuf [4]byte
	binary.BigEndian.PutUint32(lenBuf[:], uint32(len(source)))
	if _, err := proc.Stdin.Write(lenBuf[:]); err != nil {
		return nil, fmt.Errorf("wasmantlr: write to parser %q: %w", name, err)
	}
	if _, err := proc.Stdin.Write(source); err != nil {
		return nil, fmt.Errorf("wasmantlr: write source to parser %q: %w", name, err)
	}


	if _, err := io.ReadFull(proc.Stdout, lenBuf[:]); err != nil {
		return nil, fmt.Errorf("wasmantlr: read response length from %q: %w", name, err)
	}
	respLen := binary.BigEndian.Uint32(lenBuf[:])
	if respLen == 0 || respLen > 256*1024*1024 {
		return nil, fmt.Errorf("wasmantlr: invalid response length %d from %q", respLen, name)
	}

	respBuf := make([]byte, respLen)
	if _, err := io.ReadFull(proc.Stdout, respBuf); err != nil {
		return nil, fmt.Errorf("wasmantlr: read response from %q: %w", name, err)
	}

	return ParseTreeFromJSON(respBuf)
}

// Close shuts down all parser processes and the wazero runtime.
func (e *Engine) Close() error {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.closed = true

	for _, proc := range e.procs {
		if proc.Close != nil {
			proc.Close()
		}
	}

	// Wait for background goroutines before closing the runtime to avoid data races.
	for _, proc := range e.procs {
		if proc.Wait != nil {
			proc.Wait()
		}
	}

	err := e.rt.Close(e.ctx)
	if e.cache != nil {
		_ = e.cache.Close(e.ctx)
	}
	return err
}

// --- WASM instance backend ---

func (e *Engine) startWASMProc(name string, compiled wazero.CompiledModule) (*ParserProc, error) {
	stdinR, stdinW := io.Pipe()
	stdoutR, stdoutW := io.Pipe()

	modCfg := wazero.NewModuleConfig().
		WithName(name).
		WithStdin(stdinR).
		WithStdout(stdoutW).
		WithStderr(io.Discard).
		WithSysNanosleep().
		WithSysNanotime().
		WithSysWalltime().
		WithRandSource(rand.Reader).
		WithArgs(name)

	done := make(chan struct{})
	go func() {
		defer close(done)
		_, _ = e.rt.InstantiateModule(e.ctx, compiled, modCfg)
		stdoutW.Close()
	}()

	return &ParserProc{
		Stdin:  stdinW,
		Stdout: stdoutR,
		Close: func() {
			stdinW.Close()
		},
		Wait: func() {
			<-done
		},
	}, nil
}

// --- Native subprocess backend ---

func startNativeProc(binaryPath string) (*ParserProc, error) {
	cmd := exec.Command(binaryPath)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		stdin.Close()
		return nil, err
	}

	if err := cmd.Start(); err != nil {
		stdin.Close()
		return nil, err
	}

	return &ParserProc{
		Stdin:  stdin,
		Stdout: stdout,
		Close: func() {
			stdin.Close()
			if cmd.Process != nil {
				_ = cmd.Process.Kill()
			}
		},
		Wait: func() {
			_ = cmd.Wait()
		},
	}, nil
}
