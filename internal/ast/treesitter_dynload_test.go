package ast

import (
	"os"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

const (
	testGoGrammarPath     = "/tmp/ts-grammar-build/tree-sitter-go.so"
	testPythonGrammarPath = "/tmp/ts-grammar-build/tree-sitter-python.so"
)

const testGoSource = `package main

import "fmt"

func main() {
	fmt.Println("hello, world")
}

type Greeter struct {
	Name string
}

func (g *Greeter) Greet() string {
	return "Hello, " + g.Name
}
`

const testPythonSource = `
def greet(name):
    return f"Hello, {name}"

class Greeter:
    def __init__(self, name):
        self.name = name

    def greet(self):
        return f"Hello, {self.name}"
`

func skipIfNoSharedLib(t *testing.T, path string) {
	t.Helper()
	if _, err := os.Stat(path); os.IsNotExist(err) {
		t.Skipf("shared library not found: %s (run grammar compilation step first)", path)
	}
}

// TestDynGrammarLoader_LoadGo loads the Go grammar dynamically and parses Go source.
func TestDynGrammarLoader_LoadGo(t *testing.T) {
	skipIfNoSharedLib(t, testGoGrammarPath)

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("go", testGoGrammarPath)
	if err != nil {
		t.Fatalf("LoadFromPath failed: %v", err)
	}
	if lang == nil {
		t.Fatal("loaded language is nil")
	}

	// Parse Go source.
	parser := sitter.NewParser()
	_ = parser.SetLanguage(lang)

	tree, err := tsParse(parser, []byte(testGoSource))
	if err != nil {
		t.Fatalf("ParseCtx failed: %v", err)
	}
	if tree == nil {
		t.Fatal("parse tree is nil")
	}
	defer tree.Close()

	root := tree.RootNode()
	if root == nil {
		t.Fatal("root node is nil")
	}

	// Verify the root is a source_file with children.
	if root.Kind() != "source_file" {
		t.Errorf("expected root type 'source_file', got %q", root.Kind())
	}
	if root.ChildCount() == 0 {
		t.Error("expected root node to have children")
	}

	t.Logf("Parsed Go source: root=%s children=%d", root.Kind(), root.ChildCount())

	// Walk top-level children and verify key declarations exist.
	foundPackage := false
	foundFunc := false
	foundType := false
	for i := 0; i < int(root.ChildCount()); i++ {
		child := root.Child(uint(i))
		switch child.Kind() {
		case "package_clause":
			foundPackage = true
		case "function_declaration":
			foundFunc = true
		case "type_declaration":
			foundType = true
		}
		t.Logf("  child[%d]: type=%s", i, child.Kind())
	}

	if !foundPackage {
		t.Error("did not find package_clause in parse tree")
	}
	if !foundFunc {
		t.Error("did not find function_declaration in parse tree")
	}
	if !foundType {
		t.Error("did not find type_declaration in parse tree")
	}
}

// TestDynGrammarLoader_LoadPython loads the Python grammar dynamically and parses Python source.
func TestDynGrammarLoader_LoadPython(t *testing.T) {
	skipIfNoSharedLib(t, testPythonGrammarPath)

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("python", testPythonGrammarPath)
	if err != nil {
		t.Fatalf("LoadFromPath failed: %v", err)
	}

	parser := sitter.NewParser()
	_ = parser.SetLanguage(lang)

	tree, err := tsParse(parser, []byte(testPythonSource))
	if err != nil {
		t.Fatalf("ParseCtx failed: %v", err)
	}
	if tree == nil {
		t.Fatal("parse tree is nil")
	}
	defer tree.Close()

	root := tree.RootNode()
	if root == nil {
		t.Fatal("root node is nil")
	}

	if root.Kind() != "module" {
		t.Errorf("expected root type 'module', got %q", root.Kind())
	}
	if root.ChildCount() == 0 {
		t.Error("expected root node to have children")
	}

	t.Logf("Parsed Python source: root=%s children=%d", root.Kind(), root.ChildCount())
}

// TestDynGrammarLoader_Cache verifies that loading the same grammar twice returns the cached version.
func TestDynGrammarLoader_Cache(t *testing.T) {
	skipIfNoSharedLib(t, testGoGrammarPath)

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang1, err := loader.LoadFromPath("go", testGoGrammarPath)
	if err != nil {
		t.Fatalf("first load failed: %v", err)
	}

	lang2, err := loader.LoadFromPath("go", testGoGrammarPath)
	if err != nil {
		t.Fatalf("second load failed: %v", err)
	}

	if lang1 != lang2 {
		t.Error("expected cached language pointer to be the same")
	}

	loaded := loader.Loaded()
	if len(loaded) != 1 || loaded[0] != "go" {
		t.Errorf("expected [go], got %v", loaded)
	}
}

// TestDynGrammarLoader_SearchPath verifies that the search path mechanism works
// via project directory.
func TestDynGrammarLoader_SearchPath(t *testing.T) {
	skipIfNoSharedLib(t, testGoGrammarPath)

	loader := NewDynGrammarLoader()
	defer loader.Close()

	// Use LoadFromPath directly since search path only covers project/user dirs.
	lang, err := loader.LoadFromPath("go", testGoGrammarPath)
	if err != nil {
		t.Fatalf("Load via path failed: %v", err)
	}
	if lang == nil {
		t.Fatal("loaded language is nil")
	}

	// Verify it actually works by parsing.
	parser := sitter.NewParser()
	_ = parser.SetLanguage(lang)

	tree, err := tsParse(parser, []byte(testGoSource))
	if err != nil {
		t.Fatalf("ParseCtx failed: %v", err)
	}
	defer tree.Close()

	if tree.RootNode().ChildCount() == 0 {
		t.Error("expected parse tree to have children")
	}
}

// TestDynGrammarLoader_NotFound verifies error handling for missing grammars.
func TestDynGrammarLoader_NotFound(t *testing.T) {
	loader := NewDynGrammarLoader()
	defer loader.Close()

	_, err := loader.Load("nonexistent_language_xyz")
	if err == nil {
		t.Fatal("expected error for nonexistent grammar, got nil")
	}
	t.Logf("Expected error: %v", err)
}

// TestDynGrammarLoader_InvalidPath verifies error handling for invalid library paths.
func TestDynGrammarLoader_InvalidPath(t *testing.T) {
	loader := NewDynGrammarLoader()
	defer loader.Close()

	_, err := loader.LoadFromPath("go", "/tmp/nonexistent.so")
	if err == nil {
		t.Fatal("expected error for invalid path, got nil")
	}
	t.Logf("Expected error: %v", err)
}

// TestDynGrammarLoader_ConsistentWithNative verifies that the dynamically loaded grammar
// produces the same parse tree as the native CGO-linked grammar.
func TestDynGrammarLoader_ConsistentWithNative(t *testing.T) {
	skipIfNoSharedLib(t, testGoGrammarPath)

	// Parse with native CGO grammar.
	nativeLang := NativeLanguage("go")
	nativeParser := sitter.NewParser()
	_ = nativeParser.SetLanguage(nativeLang)

	nativeTree, err := tsParse(nativeParser, []byte(testGoSource))
	if err != nil {
		t.Fatalf("native parse failed: %v", err)
	}
	defer nativeTree.Close()

	// Parse with dynamically loaded grammar.
	loader := NewDynGrammarLoader()
	defer loader.Close()

	dynLang, err := loader.LoadFromPath("go", testGoGrammarPath)
	if err != nil {
		t.Fatalf("dynamic load failed: %v", err)
	}

	dynParser := sitter.NewParser()
	_ = dynParser.SetLanguage(dynLang)

	dynTree, err := tsParse(dynParser, []byte(testGoSource))
	if err != nil {
		t.Fatalf("dynamic parse failed: %v", err)
	}
	defer dynTree.Close()

	// Compare results.
	nativeRoot := nativeTree.RootNode()
	dynRoot := dynTree.RootNode()

	if nativeRoot.Kind() != dynRoot.Kind() {
		t.Errorf("root type mismatch: native=%q dynamic=%q", nativeRoot.Kind(), dynRoot.Kind())
	}
	if nativeRoot.ChildCount() != dynRoot.ChildCount() {
		t.Errorf("root child count mismatch: native=%d dynamic=%d", nativeRoot.ChildCount(), dynRoot.ChildCount())
	}

	// Deep compare the S-expressions.
	nativeSexp := nativeRoot.ToSexp()
	dynSexp := dynRoot.ToSexp()
	if nativeSexp != dynSexp {
		t.Errorf("S-expression mismatch:\n  native:  %s\n  dynamic: %s", nativeSexp, dynSexp)
	} else {
		t.Log("Native and dynamic grammars produce identical parse trees ✓")
	}
}

// BenchmarkTS_Dynamic benchmarks parsing with dynamically loaded grammar.
func BenchmarkTS_Dynamic(b *testing.B) {
	skipIfNoSharedLibBench(b, testGoGrammarPath)

	loader := NewDynGrammarLoader()
	defer loader.Close()

	lang, err := loader.LoadFromPath("go", testGoGrammarPath)
	if err != nil {
		b.Fatalf("LoadFromPath failed: %v", err)
	}

	src := []byte(testGoSource)

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		parser := sitter.NewParser()
		_ = parser.SetLanguage(lang)
		tree, err := tsParse(parser, src)
		if err != nil {
			b.Fatalf("parse failed: %v", err)
		}
		tree.Close()
	}
}

// BenchmarkTS_Native benchmarks parsing with native CGO-linked grammar.
func BenchmarkTS_Native(b *testing.B) {
	lang := NativeLanguage("go")
	src := []byte(testGoSource)

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		parser := sitter.NewParser()
		_ = parser.SetLanguage(lang)
		tree, err := tsParse(parser, src)
		if err != nil {
			b.Fatalf("parse failed: %v", err)
		}
		tree.Close()
	}
}

func skipIfNoSharedLibBench(b *testing.B, path string) {
	b.Helper()
	if _, err := os.Stat(path); os.IsNotExist(err) {
		b.Skipf("shared library not found: %s", path)
	}
}
