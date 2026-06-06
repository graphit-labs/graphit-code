package wasmantlr_test

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
)

func TestAntlrPlSqlWASM(t *testing.T) {
	wasmPath := "../grammars/antlr-plsql.wasm"
	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		t.Skipf("antlr-plsql.wasm not found at %s (run make build-antlr-grammars first): %v", wasmPath, err)
	}

	engine, err := wasmantlr.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	if err := engine.Compile("plsql", wasmBytes); err != nil {
		t.Fatalf("Compile: %v", err)
	}

	source := []byte("SELECT id, name FROM users WHERE active = 1;")

	tree, err := engine.Parse("plsql", source)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}

	if tree.Rule != "sql_script" {
		t.Errorf("expected root rule 'sql_script', got %q", tree.Rule)
	}
	if len(tree.Children) == 0 {
		t.Error("expected children in parse tree, got 0")
	}

	t.Logf("Root: %s, children: %d", tree.Rule, len(tree.Children))

	// Walk first few levels to verify structure
	for _, child := range tree.Children {
		if child.Rule != "" {
			t.Logf("  [rule] %s (line %d)", child.Rule, child.Start[0])
		} else if child.Token != "" {
			t.Logf("  [token] %s = %q", child.Token, child.Text)
		}
	}
}
