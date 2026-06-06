package ast

import (
	"encoding/binary"
	"fmt"
	"io"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
)

// AntlrWorkerModules holds per-worker ANTLR parser instances.
// NOT thread-safe — one per goroutine.
type AntlrWorkerModules struct {
	engine *wasmantlr.Engine
	procs  map[string]*wasmantlr.ParserProc
}

func NewAntlrWorkerModules(engine *wasmantlr.Engine) *AntlrWorkerModules {
	if engine == nil {
		return nil
	}
	return &AntlrWorkerModules{
		engine: engine,
		procs:  make(map[string]*wasmantlr.ParserProc),
	}
}

// Parse sends source to the worker's own ANTLR proc and returns the parse tree.
func (awm *AntlrWorkerModules) Parse(name string, source []byte) (*wasmantlr.TreeNode, error) {
	if awm == nil {
		return nil, fmt.Errorf("antlr worker modules not initialized")
	}

	proc, ok := awm.procs[name]
	if !ok {
		// Try creating a per-worker WASM instance (for compiled modules)
		var err error
		proc, err = awm.engine.NewWorkerProc(name)
		if err != nil {
			// No compiled WASM — fall back to singleton proc (native binary).
			// engine.Parse() handles mutex locking internally.
			return awm.engine.Parse(name, source)
		}
		awm.procs[name] = proc
	}

	var lenBuf [4]byte
	binary.BigEndian.PutUint32(lenBuf[:], uint32(len(source)))
	if _, err := proc.Stdin.Write(lenBuf[:]); err != nil {
		return nil, fmt.Errorf("antlr worker write %q: %w", name, err)
	}
	if _, err := proc.Stdin.Write(source); err != nil {
		return nil, fmt.Errorf("antlr worker write source %q: %w", name, err)
	}

	if _, err := io.ReadFull(proc.Stdout, lenBuf[:]); err != nil {
		return nil, fmt.Errorf("antlr worker read %q: %w", name, err)
	}
	respLen := binary.BigEndian.Uint32(lenBuf[:])
	if respLen == 0 || respLen > 256*1024*1024 {
		return nil, fmt.Errorf("antlr worker invalid response length %d from %q", respLen, name)
	}
	respBuf := make([]byte, respLen)
	if _, err := io.ReadFull(proc.Stdout, respBuf); err != nil {
		return nil, fmt.Errorf("antlr worker read response %q: %w", name, err)
	}

	return wasmantlr.ParseTreeFromJSON(respBuf)
}

func (awm *AntlrWorkerModules) Close() {
	if awm == nil {
		return
	}
	for _, proc := range awm.procs {
		if proc.Close != nil {
			proc.Close()
		}
	}
	awm.procs = nil
}
