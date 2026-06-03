package wasmts_test

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
)

// findGrammarDir locates the compiled .wasm grammars directory.
func findGrammarDir(t *testing.T) string {
	t.Helper()
	// Walk up from the test file to find internal/ast/grammars/
	_, filename, _, _ := runtime.Caller(0)
	dir := filepath.Dir(filename)
	grammarDir := filepath.Join(dir, "..", "grammars")
	if _, err := os.Stat(grammarDir); err != nil {
		t.Skipf("grammar dir not found at %s (run 'make build-grammars' first)", grammarDir)
	}
	return grammarDir
}

func loadTestLanguage(t *testing.T, engine *wasmts.Engine, lang string) *wasmts.Language {
	t.Helper()
	grammarDir := findGrammarDir(t)
	wasmPath := filepath.Join(grammarDir, "tree-sitter-"+lang+".wasm")
	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		t.Skipf("grammar %s not available: %v", lang, err)
	}

	mod, err := engine.LoadModule("tree-sitter-"+lang, wasmBytes)
	if err != nil {
		t.Fatalf("LoadModule(%s): %v", lang, err)
	}

	language, err := mod.LoadLanguage(lang)
	if err != nil {
		t.Fatalf("LoadLanguage(%s): %v", lang, err)
	}
	return language
}

func TestEngineLifecycle(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	// Load Go grammar
	lang := loadTestLanguage(t, engine, "go")

	v, err := lang.Version()
	if err != nil {
		t.Fatalf("Version: %v", err)
	}
	if v == 0 {
		t.Error("expected non-zero language version")
	}
	t.Logf("Go grammar version: %d", v)
}

func TestParseGoCode(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	lang := loadTestLanguage(t, engine, "go")

	parser, err := lang.NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}
	defer parser.Close()

	src := []byte(`package main

import "fmt"

func hello(name string) string {
	return fmt.Sprintf("Hello, %s!", name)
}

func main() {
	fmt.Println(hello("world"))
}
`)

	tree, err := parser.Parse(src)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	defer tree.Close()

	root, err := tree.RootNode()
	if err != nil {
		t.Fatalf("RootNode: %v", err)
	}

	// Root should be "source_file"
	rootType, err := root.Type()
	if err != nil {
		t.Fatalf("root.Type: %v", err)
	}
	if rootType != "source_file" {
		t.Errorf("root type = %q, want source_file", rootType)
	}

	// Should have children
	childCount, err := root.ChildCount()
	if err != nil {
		t.Fatalf("root.ChildCount: %v", err)
	}
	if childCount == 0 {
		t.Error("expected root to have children")
	}
	t.Logf("root has %d children", childCount)
}

func TestNodeMethods(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	lang := loadTestLanguage(t, engine, "go")

	parser, err := lang.NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}
	defer parser.Close()

	src := []byte(`package main

func hello() string {
	return "world"
}
`)

	tree, err := parser.Parse(src)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	defer tree.Close()

	root, err := tree.RootNode()
	if err != nil {
		t.Fatalf("RootNode: %v", err)
	}

	// Find the function_declaration (second child: package_clause, then function_declaration)
	cc, _ := root.ChildCount()
	var funcNode *wasmts.Node
	for i := 0; i < int(cc); i++ {
		child, err := root.Child(i)
		if err != nil || child == nil {
			continue
		}
		ct, _ := child.Type()
		if ct == "function_declaration" {
			funcNode = child
			break
		}
	}
	if funcNode == nil {
		t.Fatal("function_declaration not found")
	}

	// Test Type()
	ft, _ := funcNode.Type()
	if ft != "function_declaration" {
		t.Errorf("func type = %q", ft)
	}

	// Test ChildByFieldName("name")
	nameNode, err := funcNode.ChildByFieldName("name")
	if err != nil {
		t.Fatalf("ChildByFieldName(name): %v", err)
	}
	if nameNode == nil {
		t.Fatal("name node is nil")
	}
	name := nameNode.Content()
	if name != "hello" {
		t.Errorf("function name = %q, want hello", name)
	}

	// Test StartPoint / EndPoint
	sp, err := nameNode.StartPoint()
	if err != nil {
		t.Fatalf("StartPoint: %v", err)
	}
	if sp.Row != 2 {
		t.Errorf("name start row = %d, want 2", sp.Row)
	}

	ep, err := nameNode.EndPoint()
	if err != nil {
		t.Fatalf("EndPoint: %v", err)
	}
	if ep.Row != 2 {
		t.Errorf("name end row = %d, want 2", ep.Row)
	}

	// Test Parent()
	parent, err := nameNode.Parent()
	if err != nil {
		t.Fatalf("Parent: %v", err)
	}
	if parent == nil {
		t.Fatal("parent is nil")
	}
	parentType, _ := parent.Type()
	if parentType != "function_declaration" {
		t.Errorf("parent type = %q, want function_declaration", parentType)
	}

	// Test StartByte / EndByte
	sb, _ := nameNode.StartByte()
	eb, _ := nameNode.EndByte()
	if sb >= eb {
		t.Errorf("startByte=%d >= endByte=%d", sb, eb)
	}

	// Test Content matches source
	content := nameNode.Content()
	expected := string(src[sb:eb])
	if content != expected {
		t.Errorf("Content() = %q, want %q", content, expected)
	}

	t.Logf("✓ function %q at line %d, bytes [%d:%d]", name, sp.Row+1, sb, eb)
}

func TestQuery(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	lang := loadTestLanguage(t, engine, "go")

	parser, err := lang.NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}
	defer parser.Close()

	src := []byte(`package main

func hello() {}
func world() {}
func fooBar() {}
`)

	tree, err := parser.Parse(src)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	defer tree.Close()

	root, err := tree.RootNode()
	if err != nil {
		t.Fatalf("RootNode: %v", err)
	}

	// Query for function names
	pattern := `(function_declaration name: (identifier) @func.name)`
	q, err := lang.NewQuery(pattern)
	if err != nil {
		t.Fatalf("NewQuery: %v", err)
	}
	defer q.Close()

	qc, err := lang.NewQueryCursor()
	if err != nil {
		t.Fatalf("NewQueryCursor: %v", err)
	}
	defer qc.Close()

	if err := qc.Exec(q, root); err != nil {
		t.Fatalf("Exec: %v", err)
	}

	var funcNames []string
	for {
		match, ok, err := qc.NextMatch(src)
		if err != nil {
			t.Fatalf("NextMatch: %v", err)
		}
		if !ok {
			break
		}
		for _, capture := range match.Captures {
			funcNames = append(funcNames, capture.Node.Content())
		}
	}

	if len(funcNames) != 3 {
		t.Fatalf("expected 3 functions, got %d: %v", len(funcNames), funcNames)
	}
	expected := []string{"hello", "world", "fooBar"}
	for i, name := range funcNames {
		if name != expected[i] {
			t.Errorf("func[%d] = %q, want %q", i, name, expected[i])
		}
	}
	t.Logf("✓ found functions: %v", funcNames)
}

func TestParsePython(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	lang := loadTestLanguage(t, engine, "python")

	parser, err := lang.NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}
	defer parser.Close()

	src := []byte(`def greet(name):
    """Say hello."""
    return f"Hello, {name}!"

class Person:
    def __init__(self, name):
        self.name = name
`)

	tree, err := parser.Parse(src)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	defer tree.Close()

	root, err := tree.RootNode()
	if err != nil {
		t.Fatalf("RootNode: %v", err)
	}

	rootType, _ := root.Type()
	if rootType != "module" {
		t.Errorf("root type = %q, want module", rootType)
	}

	// Query for class and function names
	pattern := `[
		(function_definition name: (identifier) @name)
		(class_definition name: (identifier) @name)
	]`
	q, err := lang.NewQuery(pattern)
	if err != nil {
		t.Fatalf("NewQuery: %v", err)
	}
	defer q.Close()

	qc, err := lang.NewQueryCursor()
	if err != nil {
		t.Fatalf("NewQueryCursor: %v", err)
	}
	defer qc.Close()

	if err := qc.Exec(q, root); err != nil {
		t.Fatalf("Exec: %v", err)
	}

	var names []string
	for {
		match, ok, _ := qc.NextMatch(src)
		if !ok {
			break
		}
		for _, c := range match.Captures {
			names = append(names, c.Node.Content())
		}
	}

	t.Logf("✓ found names: %v", names)
	if len(names) < 3 {
		t.Errorf("expected at least 3 names (greet, Person, __init__), got %v", names)
	}
}

func TestListAvailableLanguages(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	grammarDir := findGrammarDir(t)
	wasmBytes, err := os.ReadFile(filepath.Join(grammarDir, "tree-sitter-go.wasm"))
	if err != nil {
		t.Skip("go grammar not available")
	}

	mod, err := engine.LoadModule("tree-sitter-go", wasmBytes)
	if err != nil {
		t.Fatalf("LoadModule: %v", err)
	}

	langs := mod.ListAvailableLanguages()
	if len(langs) == 0 {
		t.Error("expected at least 1 language")
	}
	t.Logf("available languages: %v", langs)

	found := false
	for _, l := range langs {
		if l == "go" {
			found = true
		}
	}
	if !found {
		t.Errorf("'go' not found in %v", langs)
	}
}

func TestInvalidQuery(t *testing.T) {
	engine, err := wasmts.NewEngine("")
	if err != nil {
		t.Fatalf("NewEngine: %v", err)
	}
	defer engine.Close()

	lang := loadTestLanguage(t, engine, "go")

	_, err = lang.NewQuery("(invalid_node_type_xyz @cap)")
	if err == nil {
		t.Error("expected error for invalid query pattern")
	}
	t.Logf("✓ got expected error: %v", err)
}
