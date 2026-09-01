package ast

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Disabling a language used to mean deleting its query file, which only the
// repository that ships the query files can do. ast.grammars_blacklist and
// ast.grammars_whitelist move that decision into configuration, so a consumer can
// take a language out of its own index without editing the installed runtime.

// The three grammars staged by these tests. They are invented on purpose: a real
// language's registration comes from the installed runtime, which a test must not
// depend on.
const (
	tsQueryFile = `language: fable_lang
grammar: tree-sitter-fable
extensions: [".fable"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	antlrOneQueryFile = `language: dialect_one
parser: antlr4
grammar: antlr-dialect_one
start_rule: root
extensions: [".dial"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: 'function_declaration'
`
	antlrTwoQueryFile = `language: dialect_two
parser: antlr4
grammar: antlr-dialect_two
start_rule: root
extensions: [".dial"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: 'function_declaration'
`
)

// stageFilterProject builds a project whose own queries directory declares the
// languages above, with the given ast config on its lockfile.
//
// HOME is redirected because the filter resolves the GLOBAL config file as the
// last step of the precedence chain: a real ~/.graphit/config.json on the machine
// running the tests would otherwise decide their outcome.
func stageFilterProject(t *testing.T, astCfg map[string]any, queryFiles map[string]string) string {
	t.Helper()
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	project := t.TempDir()
	if astCfg != nil {
		writeLockfileConfig(t, project, map[string]any{"ast": astCfg})
	}
	dir := filepath.Join(project, ".graphit", "ast", "queries")
	for name, body := range queryFiles {
		stageQueryFile(t, dir, name, body)
	}
	return project
}

func stageFableProject(t *testing.T, astCfg map[string]any) string {
	t.Helper()
	return stageFilterProject(t, astCfg, map[string]string{"fable.yaml": tsQueryFile})
}

func TestGrammarFilterAbsentByDefault(t *testing.T) {
	project := stageFableProject(t, nil)

	if f := grammarFilterFor(project); !f.inert() {
		t.Error("no configuration should produce an inert filter")
	}
	if !HasParserForExtensionIn(project, ".fable") {
		t.Error("a language with no filter configured must stay enabled")
	}
	if got := TreeSitterLangForExtensionIn(project, ".fable"); got != "fable_lang" {
		t.Errorf("language for .fable = %q, want fable_lang", got)
	}
}

func TestGrammarBlacklistRemovesTheExtension(t *testing.T) {
	project := stageFableProject(t, map[string]any{"grammars_blacklist": "fable_lang"})

	if HasParserForExtensionIn(project, ".fable") {
		t.Error("a blacklisted language must not claim its extensions")
	}
	if got := TreeSitterLangForExtensionIn(project, ".fable"); got != "" {
		t.Errorf("language for .fable = %q, want empty", got)
	}
}

// Every name the language answers to disables it. The three differ in this fixture —
// fable_lang / tree-sitter-fable / fable — and an entry that matched only one of
// them would make the obvious input do nothing.
func TestGrammarBlacklistMatchesLanguageGrammarAndBareGrammar(t *testing.T) {
	for _, entry := range []string{
		"fable_lang",
		"tree-sitter-fable",
		"fable",
		"FABLE_LANG",
		"  fable  ",
		"go, fable ,python",
	} {
		t.Run(entry, func(t *testing.T) {
			project := stageFableProject(t, map[string]any{"grammars_blacklist": entry})
			if HasParserForExtensionIn(project, ".fable") {
				t.Errorf("blacklist %q did not disable the language", entry)
			}
		})
	}
}

func TestGrammarBlacklistOfAnUnknownNameDisablesNothing(t *testing.T) {
	project := stageFableProject(t, map[string]any{"grammars_blacklist": "cobol,algol"})

	if !HasParserForExtensionIn(project, ".fable") {
		t.Error("an unknown name must be inert, not disable an unrelated language")
	}
}

// A non-empty whitelist is exhaustive: anything it does not name is disabled,
// including a language the project declares for itself.
func TestGrammarWhitelistExcludesEverythingElse(t *testing.T) {
	project := stageFilterProject(t, map[string]any{"grammars_whitelist": "dialect_one"},
		map[string]string{"fable.yaml": tsQueryFile, "dialect_one.yaml": antlrOneQueryFile})

	if HasParserForExtensionIn(project, ".fable") {
		t.Error("a language absent from a non-empty whitelist must be disabled")
	}
	if !HasParserForExtensionIn(project, ".dial") {
		t.Error("the whitelisted language must stay enabled")
	}
}

func TestGrammarBlacklistWinsOverWhitelist(t *testing.T) {
	project := stageFilterProject(t, map[string]any{
		"grammars_whitelist": "fable_lang,dialect_one",
		"grammars_blacklist": "fable",
	}, map[string]string{"fable.yaml": tsQueryFile, "dialect_one.yaml": antlrOneQueryFile})

	if HasParserForExtensionIn(project, ".fable") {
		t.Error("a name in both lists must be disabled")
	}
	if !HasParserForExtensionIn(project, ".dial") {
		t.Error("the whitelisted language that is not blacklisted must stay enabled")
	}
}

// An extension claimed by several ANTLR dialects narrows to the enabled ones
// rather than being rejected wholesale: antlrExtMap holds a list per extension.
func TestGrammarBlacklistNarrowsAntlrDialectsWithoutDroppingTheExtension(t *testing.T) {
	project := stageFilterProject(t, map[string]any{"grammars_blacklist": "antlr-dialect_two"},
		map[string]string{"dialect_one.yaml": antlrOneQueryFile, "dialect_two.yaml": antlrTwoQueryFile})

	if !HasParserForExtensionIn(project, ".dial") {
		t.Fatal("one dialect off must leave the extension claimed by the other")
	}
	if got := resolveQueriesForLang(project, "dialect_two", ".dial"); len(got) != 0 {
		t.Errorf("disabled dialect resolved %d query files, want 0", len(got))
	}
	if got := resolveQueriesForLang(project, "dialect_one", ".dial"); len(got) != 1 {
		t.Errorf("enabled dialect resolved %d query files, want 1", len(got))
	}
}

// The queries have to disappear too, not only the extension: a parse that reached
// a disabled language by any other route must extract nothing rather than fall
// through to the level below.
func TestDisabledLanguageResolvesNoQueries(t *testing.T) {
	project := stageFableProject(t, map[string]any{"grammars_blacklist": "fable_lang"})

	if got := resolveQueriesForLang(project, "fable_lang", ".fable"); len(got) != 0 {
		t.Errorf("resolved %d query files for a disabled language, want 0", len(got))
	}
}

// An explicit --grammar override does not revive a disabled grammar: discovery
// would have dropped its files anyway, so honouring it would only move the
// failure to a place that looks like a bug.
//
// The query file is staged in the USER directory, not the project's: tsGrammarMap
// is built from the runtime and user levels alone, so a project-declared grammar
// is not reachable by name at all and the test would pass for the wrong reason.
func TestGrammarOverrideCannotReviveADisabledGrammar(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	stageQueryFile(t, filepath.Join(home, brand.DotDir(), "ast", "queries"), "fable.yaml", tsQueryFile)
	project := t.TempDir()
	parser := &TreeSitterParser{projectDir: project}
	target := filepath.Join(project, "x.fable")

	// The global tables are refreshed lazily, by the extension lookups rather than
	// by InvalidateQueryCaches — which rebuilds them from what was last read. So
	// this call is what puts the user-level grammar into tsGrammarMap, and it is
	// also the precondition: the grammar has to be reachable by name for the
	// assertion below to mean anything.
	if !HasParserForExtensionIn(project, ".fable") {
		t.Fatal("precondition: the user-level grammar must be registered")
	}
	if _, err := parser.ParseWithGrammar(target, "tree-sitter-fable", false, ParseOptions{}); err != nil &&
		strings.Contains(err.Error(), "unknown tree-sitter grammar") {
		t.Fatalf("precondition: the grammar must be reachable by name, got %v", err)
	}

	writeLockfileConfig(t, project, map[string]any{
		"ast": map[string]any{"grammars_blacklist": "fable"},
	})
	InvalidateQueryCaches()

	_, err := parser.ParseWithGrammar(target, "tree-sitter-fable", false, ParseOptions{})
	if err == nil || !strings.Contains(err.Error(), "disabled by configuration") {
		t.Fatalf("error = %v, want it to name the configuration", err)
	}
}

// The key has to land on a process that already resolved it — the daemon runs for
// days, and a config change it only saw at startup would be invisible.
func TestGrammarFilterChangeIsPickedUp(t *testing.T) {
	project := stageFableProject(t, nil)

	if !HasParserForExtensionIn(project, ".fable") {
		t.Fatal("precondition: the language starts enabled")
	}

	writeLockfileConfig(t, project, map[string]any{
		"ast": map[string]any{"grammars_blacklist": "fable_lang"},
	})
	InvalidateQueryCaches()

	if HasParserForExtensionIn(project, ".fable") {
		t.Error("the new blacklist did not take effect")
	}
}

// The environment variable is the layer above the lockfile, and it is what a
// one-off run uses. Resolution is config.ResolveConfig's, so this asserts the key
// name rather than the mechanism.
func TestGrammarBlacklistFromEnvironment(t *testing.T) {
	project := stageFableProject(t, nil)

	t.Setenv("GRAPHIT_AST_GRAMMARS_BLACKLIST", "fable")
	InvalidateQueryCaches()

	if HasParserForExtensionIn(project, ".fable") {
		t.Error("GRAPHIT_AST_GRAMMARS_BLACKLIST did not disable the language")
	}
}

func TestGrammarAliasesCoverTheDefaultedGrammarName(t *testing.T) {
	cases := []struct {
		name string
		qf   ExternalQueryFile
		want string
	}{
		{"declared grammar", ExternalQueryFile{Language: "yaml", Grammar: "tree-sitter-yaml"}, "tree-sitter-yaml"},
		{"defaulted tree-sitter", ExternalQueryFile{Language: "go"}, "tree-sitter-go"},
		{"defaulted antlr", ExternalQueryFile{Language: "plsql", Parser: "antlr4"}, "antlr-plsql"},
		{"no language", ExternalQueryFile{}, ""},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := effectiveGrammarName(tc.qf); got != tc.want {
				t.Errorf("effectiveGrammarName = %q, want %q", got, tc.want)
			}
		})
	}

	filter := grammarFilter{blacklist: map[string]bool{"yaml": true}}
	if filter.allowsFile(ExternalQueryFile{Language: "yaml", Grammar: "tree-sitter-yaml"}) {
		t.Error("the bare grammar name must match the prefixed one")
	}
	if !filter.allowsFile(ExternalQueryFile{Language: "yaml_other", Grammar: "tree-sitter-yamlish"}) {
		t.Error("the match is exact per alias, not a substring")
	}
}
