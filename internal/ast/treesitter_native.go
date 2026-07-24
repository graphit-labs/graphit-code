package ast

import (
	"strings"
	"unsafe"

	ts "github.com/tree-sitter/go-tree-sitter"

	// Grammar modules that ship their own Go bindings (bindings/go).
	tree_sitter_bash "github.com/tree-sitter/tree-sitter-bash/bindings/go"
	tree_sitter_c_sharp "github.com/tree-sitter/tree-sitter-c-sharp/bindings/go"
	tree_sitter_c "github.com/tree-sitter/tree-sitter-c/bindings/go"
	tree_sitter_cpp "github.com/tree-sitter/tree-sitter-cpp/bindings/go"
	tree_sitter_css "github.com/tree-sitter/tree-sitter-css/bindings/go"
	tree_sitter_go "github.com/tree-sitter/tree-sitter-go/bindings/go"
	tree_sitter_haskell "github.com/tree-sitter/tree-sitter-haskell/bindings/go"
	tree_sitter_html "github.com/tree-sitter/tree-sitter-html/bindings/go"
	tree_sitter_java "github.com/tree-sitter/tree-sitter-java/bindings/go"
	tree_sitter_javascript "github.com/tree-sitter/tree-sitter-javascript/bindings/go"
	tree_sitter_json "github.com/tree-sitter/tree-sitter-json/bindings/go"
	tree_sitter_julia "github.com/tree-sitter/tree-sitter-julia/bindings/go"
	tree_sitter_php "github.com/tree-sitter/tree-sitter-php/bindings/go"
	tree_sitter_python "github.com/tree-sitter/tree-sitter-python/bindings/go"
	tree_sitter_ruby "github.com/tree-sitter/tree-sitter-ruby/bindings/go"
	tree_sitter_rust "github.com/tree-sitter/tree-sitter-rust/bindings/go"
	tree_sitter_scala "github.com/tree-sitter/tree-sitter-scala/bindings/go"
	tree_sitter_typescript "github.com/tree-sitter/tree-sitter-typescript/bindings/go"

	tree_sitter_hcl "github.com/tree-sitter-grammars/tree-sitter-hcl/bindings/go"
	tree_sitter_lua "github.com/tree-sitter-grammars/tree-sitter-lua/bindings/go"
	tree_sitter_toml "github.com/tree-sitter-grammars/tree-sitter-toml/bindings/go"
	tree_sitter_xml "github.com/tree-sitter-grammars/tree-sitter-xml/bindings/go"
	tree_sitter_yaml "github.com/tree-sitter-grammars/tree-sitter-yaml/bindings/go"
	tree_sitter_zig "github.com/tree-sitter-grammars/tree-sitter-zig/bindings/go"

	// Vendored grammars (parser.c committed under internal/ast/treesitter/<lang>).
	tsClojure "github.com/graphit-labs/graphit-code/internal/ast/treesitter/clojure"
	tsDart "github.com/graphit-labs/graphit-code/internal/ast/treesitter/dart"
	tsDockerfile "github.com/graphit-labs/graphit-code/internal/ast/treesitter/dockerfile"
	tsElixir "github.com/graphit-labs/graphit-code/internal/ast/treesitter/elixir"
	tsGraphQL "github.com/graphit-labs/graphit-code/internal/ast/treesitter/graphql"
	tsGroovy "github.com/graphit-labs/graphit-code/internal/ast/treesitter/groovy"
	tsKotlin "github.com/graphit-labs/graphit-code/internal/ast/treesitter/kotlin"
	tsMarkdown "github.com/graphit-labs/graphit-code/internal/ast/treesitter/markdown"
	tsObjC "github.com/graphit-labs/graphit-code/internal/ast/treesitter/objc"
	tsProto "github.com/graphit-labs/graphit-code/internal/ast/treesitter/proto"
	tsR "github.com/graphit-labs/graphit-code/internal/ast/treesitter/r"
	tsSQL "github.com/graphit-labs/graphit-code/internal/ast/treesitter/sql"
	tsSvelte "github.com/graphit-labs/graphit-code/internal/ast/treesitter/svelte"
	tsSwift "github.com/graphit-labs/graphit-code/internal/ast/treesitter/swift"
)

// nativeGrammars maps language names to a function returning the raw tree-sitter
// language pointer. Keys match the grammar YAML conventions: "go" not "golang",
// "c-sharp" not "csharp". Pointers are wrapped by NativeLanguage with the
// official tree-sitter runtime (github.com/tree-sitter/go-tree-sitter).
var nativeGrammars = map[string]func() unsafe.Pointer{
	"bash":       tree_sitter_bash.Language,
	"c":          tree_sitter_c.Language,
	"cpp":        tree_sitter_cpp.Language,
	"c-sharp":    tree_sitter_c_sharp.Language,
	"css":        tree_sitter_css.Language,
	"go":         tree_sitter_go.Language,
	"haskell":    tree_sitter_haskell.Language,
	"html":       tree_sitter_html.Language,
	"java":       tree_sitter_java.Language,
	"javascript": tree_sitter_javascript.Language,
	"json":       tree_sitter_json.Language,
	"julia":      tree_sitter_julia.Language,
	"php":        tree_sitter_php.LanguagePHP,
	"python":     tree_sitter_python.Language,
	"ruby":       tree_sitter_ruby.Language,
	"rust":       tree_sitter_rust.Language,
	"scala":      tree_sitter_scala.Language,
	"typescript": tree_sitter_typescript.LanguageTypescript,
	"tsx":        tree_sitter_typescript.LanguageTSX,
	"hcl":        tree_sitter_hcl.Language,
	"lua":        tree_sitter_lua.Language,
	"toml":       tree_sitter_toml.Language,
	"xml":        tree_sitter_xml.LanguageXML,
	"yaml":       tree_sitter_yaml.Language,
	"zig":        tree_sitter_zig.Language,
	// Vendored
	"clojure":    tsClojure.Language,
	"dart":       tsDart.Language,
	"dockerfile": tsDockerfile.Language,
	"elixir":     tsElixir.Language,
	"graphql":    tsGraphQL.Language,
	"groovy":     tsGroovy.Language,
	"kotlin":     tsKotlin.Language,
	"markdown":   tsMarkdown.Language,
	"objc":       tsObjC.Language,
	"proto":      tsProto.Language,
	"r":          tsR.Language,
	"sql":        tsSQL.Language,
	"svelte":     tsSvelte.Language,
	"swift":      tsSwift.Language,
}

// NativeLanguage returns a natively compiled grammar, or nil if unavailable.
// It normalises underscores to hyphens so that both "c_sharp" (grammar YAML)
// and "c-sharp" (map key) resolve correctly.
func NativeLanguage(lang string) *ts.Language {
	if fn, ok := nativeGrammars[lang]; ok {
		return ts.NewLanguage(fn())
	}
	// Try underscore↔hyphen normalisation (e.g. "c_sharp" → "c-sharp").
	normalised := strings.ReplaceAll(lang, "_", "-")
	if normalised != lang {
		if fn, ok := nativeGrammars[normalised]; ok {
			return ts.NewLanguage(fn())
		}
	}
	return nil
}

// HasNativeGrammar reports whether a natively compiled grammar exists for lang.
func HasNativeGrammar(lang string) bool {
	_, ok := nativeGrammars[lang]
	return ok
}
