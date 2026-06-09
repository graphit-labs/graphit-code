package main

import (
	"fmt"
	"os"

	"github.com/bytecodealliance/wasmtime-go/v21"
)

func main() {
	wasmBytes, err := os.ReadFile("internal/ast/grammars/tree-sitter-go.wasm")
	if err != nil {
		fmt.Printf("ReadFile error: %v\n", err)
		return
	}

	config := wasmtime.NewConfig()
	engine := wasmtime.NewEngineWithConfig(config)

	module, err := wasmtime.NewModule(engine, wasmBytes)
	if err != nil {
		fmt.Printf("NewModule error: %v\n", err)
		return
	}

	fmt.Printf("Module exports (count: %d):\n", len(module.Exports()))
	for _, exp := range module.Exports() {
		fmt.Printf("  - %s\n", exp.Name())
	}
}









