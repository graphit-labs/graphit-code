package ast



import (
	"strings"

	sitter "github.com/smacker/go-tree-sitter"


	"github.com/smacker/go-tree-sitter/bash"
	"github.com/smacker/go-tree-sitter/c"
	"github.com/smacker/go-tree-sitter/cpp"
	"github.com/smacker/go-tree-sitter/css"
	"github.com/smacker/go-tree-sitter/csharp"
	"github.com/smacker/go-tree-sitter/dockerfile"
	"github.com/smacker/go-tree-sitter/elixir"
	"github.com/smacker/go-tree-sitter/golang"
	"github.com/smacker/go-tree-sitter/groovy"
	"github.com/smacker/go-tree-sitter/hcl"
	"github.com/smacker/go-tree-sitter/html"
	"github.com/smacker/go-tree-sitter/java"
	"github.com/smacker/go-tree-sitter/javascript"
	"github.com/smacker/go-tree-sitter/kotlin"
	"github.com/smacker/go-tree-sitter/lua"
	"github.com/smacker/go-tree-sitter/markdown/tree-sitter-markdown"
	"github.com/smacker/go-tree-sitter/php"
	"github.com/smacker/go-tree-sitter/protobuf"
	"github.com/smacker/go-tree-sitter/python"
	"github.com/smacker/go-tree-sitter/ruby"
	"github.com/smacker/go-tree-sitter/rust"
	"github.com/smacker/go-tree-sitter/scala"
	"github.com/smacker/go-tree-sitter/sql"
	"github.com/smacker/go-tree-sitter/svelte"
	"github.com/smacker/go-tree-sitter/swift"
	"github.com/smacker/go-tree-sitter/toml"
	tsTypescript "github.com/smacker/go-tree-sitter/typescript/typescript"
	tsTsx "github.com/smacker/go-tree-sitter/typescript/tsx"
	"github.com/smacker/go-tree-sitter/yaml"


	tsClojure "github.com/graphit-labs/graphit-code/internal/ast/treesitter/clojure"
	tsDart "github.com/graphit-labs/graphit-code/internal/ast/treesitter/dart"
	tsGraphQL "github.com/graphit-labs/graphit-code/internal/ast/treesitter/graphql"
	tsHaskell "github.com/graphit-labs/graphit-code/internal/ast/treesitter/haskell"
	tsJSON "github.com/graphit-labs/graphit-code/internal/ast/treesitter/json"
	tsJulia "github.com/graphit-labs/graphit-code/internal/ast/treesitter/julia"
	tsObjC "github.com/graphit-labs/graphit-code/internal/ast/treesitter/objc"
	tsR "github.com/graphit-labs/graphit-code/internal/ast/treesitter/r"
	tsXML "github.com/graphit-labs/graphit-code/internal/ast/treesitter/xml"
	tsZig "github.com/graphit-labs/graphit-code/internal/ast/treesitter/zig"
)

// nativeGrammars maps language names to their GetLanguage() functions.
// Keys match the grammar YAML conventions: "go" not "golang", "c-sharp" not "csharp".
var nativeGrammars = map[string]func() *sitter.Language{

	"bash":       bash.GetLanguage,
	"c":          c.GetLanguage,
	"cpp":        cpp.GetLanguage,
	"c-sharp":    csharp.GetLanguage,
	"css":        css.GetLanguage,
	"dockerfile": dockerfile.GetLanguage,
	"elixir":     elixir.GetLanguage,
	"go":         golang.GetLanguage,
	"groovy":     groovy.GetLanguage,
	"hcl":        hcl.GetLanguage,
	"html":       html.GetLanguage,
	"java":       java.GetLanguage,
	"javascript": javascript.GetLanguage,
	"kotlin":     kotlin.GetLanguage,
	"lua":        lua.GetLanguage,
	"markdown":   tree_sitter_markdown.GetLanguage,
	"php":        php.GetLanguage,
	"proto":      protobuf.GetLanguage,
	"python":     python.GetLanguage,
	"ruby":       ruby.GetLanguage,
	"rust":       rust.GetLanguage,
	"scala":      scala.GetLanguage,
	"sql":        sql.GetLanguage,
	"svelte":     svelte.GetLanguage,
	"swift":      swift.GetLanguage,
	"toml":       toml.GetLanguage,
	"typescript": tsTypescript.GetLanguage,
	"tsx":        tsTsx.GetLanguage,
	"yaml":       yaml.GetLanguage,


	"json":    tsJSON.GetLanguage,
	"xml":     tsXML.GetLanguage,
	"zig":     tsZig.GetLanguage,
	"haskell": tsHaskell.GetLanguage,
	"julia":   tsJulia.GetLanguage,
	"dart":    tsDart.GetLanguage,


	"clojure": tsClojure.GetLanguage,
	"graphql": tsGraphQL.GetLanguage,
	"objc":    tsObjC.GetLanguage,
	"r":       tsR.GetLanguage,
}

// NativeLanguage returns a natively compiled grammar, or nil if unavailable.
// It normalises underscores to hyphens so that both "c_sharp" (grammar YAML)
// and "c-sharp" (map key) resolve correctly.
func NativeLanguage(lang string) *sitter.Language {
	if fn, ok := nativeGrammars[lang]; ok {
		return fn()
	}
	// Try underscore↔hyphen normalisation (e.g. "c_sharp" → "c-sharp").
	normalised := strings.ReplaceAll(lang, "_", "-")
	if normalised != lang {
		if fn, ok := nativeGrammars[normalised]; ok {
			return fn()
		}
	}
	return nil
}


func HasNativeGrammar(lang string) bool {
	_, ok := nativeGrammars[lang]
	return ok
}
