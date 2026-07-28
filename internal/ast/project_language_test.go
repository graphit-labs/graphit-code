package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// A query file dropped into a project's own .graphit/ast/queries must be able to
// introduce a language the installed runtime has never heard of.
//
// It could not. A query file does two separate jobs — it declares which
// extensions it handles, and it supplies the patterns that extract entities — and
// the two were read from different places. The patterns went through
// resolveQueriesForLang, which prefers the project's copy. The extension
// declaration was only ever read by initTsExtMap and initAntlrExtMap, at package
// init, out of the installed runtime alone. So a project file could override a
// language the runtime already declared, and the `extensions:` line in a file for
// a new language was inert: TreeSitterParser.Parse rejected the extension before
// anything project-level was consulted, and the file was never opened.
//
// The failure was also misleading — "no grammar for .mylang" reads as a missing
// grammar, when the grammar can be present and it is the declaration that was
// dropped.
//
// stageProjectLanguage writes a query file for an invented language under a
// project directory, borrowing a grammar that is actually available so the test
// measures the registration path and not grammar installation.
func stageProjectLanguage(t *testing.T, ext, langName string) string {
	t.Helper()

	if lang, err := resolveTreeSitterLang("go", "tree-sitter-go"); err != nil || lang == nil {
		t.Skipf("go grammar unavailable: %v", err)
	}

	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	yaml := "language: " + langName + "\n" +
		"grammar: tree-sitter-go\n" +
		"extensions: [\"" + ext + "\"]\n" +
		"declaration_types: [\"function_declaration\"]\n" +
		"queries:\n" +
		"  - data_key: functions\n" +
		"    graph_label: Function\n" +
		"    pattern: '(function_declaration name: (identifier) @name)'\n"
	if err := os.WriteFile(filepath.Join(qdir, langName+".yaml"), []byte(yaml), 0o644); err != nil {
		t.Fatal(err)
	}
	return projectDir
}

func TestProjectQueryFileRegistersItsExtension(t *testing.T) {
	const ext, langName = ".mylang", "mylang"
	projectDir := stageProjectLanguage(t, ext, langName)

	if HasParserForExtension(ext) {
		t.Fatalf("%s is registered globally — pick an extension no runtime declares", ext)
	}
	if !HasParserForExtensionIn(projectDir, ext) {
		t.Errorf("a project query file declaring %s does not make it parseable", ext)
	}
	if got := TreeSitterLangForExtensionIn(projectDir, ext); got != langName {
		t.Errorf("language for %s = %q, want %q", ext, got, langName)
	}
}

// Registration is worth nothing if discovery drops the file first: collectFiles
// and the watcher both filter by extension before any parser is asked.
func TestProjectLanguageIsDiscoverable(t *testing.T) {
	const ext, langName = ".mylang2", "mylang2"
	projectDir := stageProjectLanguage(t, ext, langName)

	srcPath := filepath.Join(projectDir, "sample"+ext)
	if err := os.WriteFile(srcPath, []byte("package p\n\nfunc Descoberta() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	files, err := collectFiles(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, f := range files {
		if filepath.Base(f) == "sample"+ext {
			found = true
		}
	}
	if !found {
		t.Errorf("discovery skipped sample%s — a project language is registered but never "+
			"reaches the parser", ext)
	}
}

// End to end: the invented language actually parses and yields entities.
func TestProjectLanguageParses(t *testing.T) {
	const ext, langName = ".mylang3", "mylang3"
	projectDir := stageProjectLanguage(t, ext, langName)

	srcPath := filepath.Join(projectDir, "sample"+ext)
	if err := os.WriteFile(srcPath,
		[]byte("package p\n\n// Doc for Alfa.\nfunc Alfa() {}\n\nfunc Beta() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	pf, err := NewCompositeParser(projectDir, nil).Parse(srcPath, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if pf == nil || pf.EntityCount() == 0 {
		t.Fatalf("no entities from a project-declared language")
	}

	names := map[string]string{}
	for _, ents := range pf.Entities {
		for _, e := range ents {
			names[e.Name] = e.Docstring
		}
	}
	for _, want := range []string{"Alfa", "Beta"} {
		if _, ok := names[want]; !ok {
			t.Errorf("entity %q missing; got %v", want, names)
		}
	}
	if names["Alfa"] != "Doc for Alfa." {
		t.Errorf("docstring for Alfa = %q, want %q", names["Alfa"], "Doc for Alfa.")
	}
}
