package ast

import (
	"os"
	"testing"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

func sidecarBin(t testing.TB) string {
	t.Helper()
	bin := os.Getenv("ANTLR_SIDECAR_BIN")
	if bin == "" {
		t.Skip("ANTLR_SIDECAR_BIN not set — skipping sidecar test")
	}
	if _, err := os.Stat(bin); err != nil {
		t.Fatalf("sidecar binary not found at %s: %v", bin, err)
	}
	return bin
}

const samplePLSQL = `
CREATE OR REPLACE PROCEDURE greet_user(p_name IN VARCHAR2) IS
BEGIN
    DBMS_OUTPUT.PUT_LINE('Hello, ' || p_name || '!');
END greet_user;
/

CREATE OR REPLACE FUNCTION add_numbers(a IN NUMBER, b IN NUMBER) RETURN NUMBER IS
BEGIN
    RETURN a + b;
END add_numbers;
/
`

// TestSidecarDriver_PLSQL verifies end-to-end sidecar parsing: build the
// binary, start it, send PL/SQL source, and verify we get a valid TreeNode back.
func TestSidecarDriver_PLSQL(t *testing.T) {
	bin := sidecarBin(t)

	drv := NewSidecarDriver(bin, "plsql", 1)
	defer drv.Close()

	tree, err := drv.Parse([]byte(samplePLSQL))
	if err != nil {
		t.Fatalf("Parse failed: %v", err)
	}
	if tree == nil {
		t.Fatal("Parse returned nil tree")
	}
	if !tree.IsRule() {
		t.Fatalf("expected rule node at root, got token=%q", tree.Token)
	}
	if tree.Rule == "" {
		t.Fatal("root node has empty rule name")
	}
	if len(tree.Children) == 0 {
		t.Fatal("root node has no children")
	}

	var terminalCount int
	countTerminals(tree, &terminalCount)
	if terminalCount == 0 {
		t.Fatal("no terminal nodes found in tree")
	}
	t.Logf("PL/SQL tree: root=%q, children=%d, terminals=%d",
		tree.Rule, len(tree.Children), terminalCount)
}

func countTerminals(n *antlrcommon.TreeNode, count *int) {
	if n.IsTerminal() {
		*count++
		return
	}
	for _, c := range n.Children {
		countTerminals(c, count)
	}
}

func TestSidecarDriver_MultipleRequests(t *testing.T) {
	bin := sidecarBin(t)

	drv := NewSidecarDriver(bin, "plsql", 1)
	defer drv.Close()

	for i := 0; i < 5; i++ {
		tree, err := drv.Parse([]byte(samplePLSQL))
		if err != nil {
			t.Fatalf("request %d failed: %v", i, err)
		}
		if tree == nil || !tree.IsRule() {
			t.Fatalf("request %d: invalid tree", i)
		}
	}
	t.Log("5 sequential requests completed successfully on single process")
}

func TestSidecarDriver_UnknownGrammar(t *testing.T) {
	bin := sidecarBin(t)

	drv := NewSidecarDriver(bin, "nonexistent-grammar", 1)
	defer drv.Close()

	_, err := drv.Parse([]byte("SELECT 1"))
	if err == nil {
		t.Fatal("expected error for unknown grammar, got nil")
	}
	t.Logf("correctly returned error: %v", err)
}

func BenchmarkANTLR_Sidecar_PLSQL(b *testing.B) {
	bin := sidecarBin(b)

	drv := NewSidecarDriver(bin, "plsql", 1)
	defer drv.Close()

	src := []byte(samplePLSQL)

	if _, err := drv.Parse(src); err != nil {
		b.Fatalf("warmup failed: %v", err)
	}

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		tree, err := drv.Parse(src)
		if err != nil {
			b.Fatalf("iteration %d: %v", i, err)
		}
		if tree == nil {
			b.Fatal("nil tree")
		}
	}
}

func BenchmarkANTLR_Sidecar_PLSQL_Pooled(b *testing.B) {
	bin := sidecarBin(b)

	const poolSize = 4
	drv := NewSidecarDriver(bin, "plsql", poolSize)
	defer drv.Close()

	src := []byte(samplePLSQL)

	for i := 0; i < poolSize; i++ {
		if _, err := drv.Parse(src); err != nil {
			b.Fatalf("warmup %d failed: %v", i, err)
		}
	}

	b.ResetTimer()
	b.ReportAllocs()
	b.RunParallel(func(pb *testing.PB) {
		for pb.Next() {
			tree, err := drv.Parse(src)
			if err != nil {
				b.Fatalf("parallel parse: %v", err)
			}
			if tree == nil {
				b.Fatal("nil tree")
			}
		}
	})
}
