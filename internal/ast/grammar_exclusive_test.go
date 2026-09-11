package ast

import (
	"os"
	"path/filepath"
	"testing"
)

const exclusiveAntlrQueryFile = `language: dialect_excl
parser: antlr4
grammar: antlr-dialect_excl
start_rule: root
extensions: [".dial", ".excl"]
exclusive: true
queries:
  - data_key: functions
    graph_label: Function
    pattern: 'function_declaration'
`

const exclusiveTsQueryFile = `language: fable_excl
grammar: tree-sitter-fable_excl
extensions: [".fable"]
exclusive: true
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`

func TestExclusiveGrammarDoesNotClaimItsExtensions(t *testing.T) {
	project := stageFilterProject(t, nil, map[string]string{
		"dialect_excl.yaml": exclusiveAntlrQueryFile,
		"fable_excl.yaml":   exclusiveTsQueryFile,
	})

	for _, ext := range []string{".dial", ".excl", ".fable"} {
		if HasParserForExtensionIn(project, ext) {
			t.Errorf("%s: an exclusive grammar must not claim its extensions", ext)
		}
	}
	if HasAntlrForExtensionIn(project, ".dial") {
		t.Error(".dial: exclusive ANTLR grammar reachable by extension")
	}
	if HasTreeSitterForExtensionIn(project, ".fable") {
		t.Error(".fable: exclusive tree-sitter grammar reachable by extension")
	}
}

// The non-exclusive dialect keeps the extension both of them claim. Exclusivity
// is per grammar, not per extension.
func TestExclusiveGrammarDoesNotTakeTheExtensionFromANormalOne(t *testing.T) {
	project := stageFilterProject(t, nil, map[string]string{
		"dialect_excl.yaml": exclusiveAntlrQueryFile,
		"dialect_one.yaml":  antlrOneQueryFile,
	})

	if !HasParserForExtensionIn(project, ".dial") {
		t.Fatal(".dial: the non-exclusive dialect must still claim it")
	}
	for _, cfg := range enabledAntlrConfigsFor(project, ".dial") {
		if cfg.Exclusive {
			t.Errorf(".dial: exclusive %s is a fallback candidate", cfg.Grammar)
		}
	}
}

// The override is the door that stays open: an extension bound to an exclusive
// grammar by ast.grammar is discovered again, and only for that extension.
func TestGrammarOverrideRestoresAnExclusiveExtension(t *testing.T) {
	project := stageFilterProject(t,
		map[string]any{"grammar": ".dial=antlr-dialect_excl"},
		map[string]string{"dialect_excl.yaml": exclusiveAntlrQueryFile},
	)

	if !HasParserForExtensionIn(project, ".dial") {
		t.Error(".dial: an extension mapped to an exclusive grammar must be discovered")
	}
	if HasParserForExtensionIn(project, ".excl") {
		t.Error(".excl: the override names .dial only")
	}
}

// A grammar the whitelist/blacklist disabled stays disabled, override or not:
// discovery must not hand the pipeline a file whose parse would be refused.
func TestGrammarOverrideDoesNotRestoreADisabledGrammar(t *testing.T) {
	project := stageFilterProject(t,
		map[string]any{
			"grammar":            ".dial=antlr-dialect_excl",
			"grammars_blacklist": "dialect_excl",
		},
		map[string]string{"dialect_excl.yaml": exclusiveAntlrQueryFile},
	)

	if HasParserForExtensionIn(project, ".dial") {
		t.Error("a blacklisted grammar must stay disabled under an override")
	}
}

func TestGrammarOverrideToAnUnknownGrammarClaimsNothing(t *testing.T) {
	project := stageFilterProject(t,
		map[string]any{"grammar": ".dial=antlr-does_not_exist"},
		map[string]string{"dialect_excl.yaml": exclusiveAntlrQueryFile},
	)

	if HasParserForExtensionIn(project, ".dial") {
		t.Error("an override naming no registered grammar must not claim the extension")
	}
}

// merge: true means the upper file speaks about what it restates. Silence about
// `exclusive` inherits the base's answer rather than switching it off.
func TestMergingQueryFileInheritsExclusive(t *testing.T) {
	base := ExternalQueryFile{Language: "dialect_excl", Parser: "antlr4", Exclusive: true}

	silent := mergeQueryFile(base, ExternalQueryFile{Language: "dialect_excl", Merge: true})
	if !silent.Exclusive {
		t.Error("a merging file silent about exclusive must inherit it")
	}

	restated := mergeQueryFile(ExternalQueryFile{Language: "d"}, ExternalQueryFile{Language: "d", Merge: true, Exclusive: true})
	if !restated.Exclusive {
		t.Error("a merging file declaring exclusive must keep it")
	}
}

func TestShippedSQLDialectsAreExclusive(t *testing.T) {
	reloadRuntimeTables := func() {
		InvalidateQueryCaches()
		initTsExtMap()
	}
	reloadRuntimeTables()
	t.Cleanup(reloadRuntimeTables)

	if !HasTreeSitterForExtensionIn("", ".sql") {
		t.Fatal(".sql must still resolve to tree-sitter-sql")
	}
	if HasAntlrForExtensionIn("", ".sql") {
		t.Error(".sql must have no ANTLR fallback dialect left")
	}

	for _, ext := range []string{".pks", ".pkb", ".prc", ".db2", ".tsql", ".pgsql", ".plpgsql"} {
		if HasParserForExtensionIn("", ext) {
			t.Errorf("%s: an exclusive dialect must not claim it without an override", ext)
		}
	}

	for _, grammar := range []string{
		"antlr-plsql", "antlr-postgresql", "antlr-db2", "antlr-tsql", "tree-sitter-plpgsql",
	} {
		if !grammarKnownIn("", grammar) {
			t.Errorf("%s: an exclusive grammar must stay reachable by name", grammar)
		}
	}
}

// The PL/pgSQL splice must survive exclusivity. A CREATE FUNCTION whose LANGUAGE
// clause says plpgsql has its dollar-quoted body re-parsed with the real PL/pgSQL
// tree-sitter grammar and spliced into the ANTLR tree
// (internal/ast/antlr/postgresql/plpgsql_splice.go), and postgresql.yaml's own
// queries and complexity: block are what read the result.
//
// That path binds the grammar at compile time and never consults the extension
// tables, so `exclusive: true` cannot reach it — but "cannot reach it" is a claim
// about code, and this is the claim exercised end to end: through the override a
// user actually writes, from discovery to the entities.
func TestPostgresOverrideKeepsThePlpgsqlSplice(t *testing.T) {
	reloadRuntimeTables := func() {
		InvalidateQueryCaches()
		initTsExtMap()
	}
	reloadRuntimeTables()
	t.Cleanup(reloadRuntimeTables)

	project := t.TempDir()
	writeLockfileConfig(t, project, map[string]any{
		"ast": map[string]any{"grammar": ".sql=antlr-postgresql"},
	})

	src := `
CREATE FUNCTION f(x INTEGER) RETURNS INTEGER AS $$
DECLARE
  spliced_local INTEGER;
BEGIN
  IF x > 0 THEN
    RETURN x;
  END IF;
  RETURN 0;
END;
$$ LANGUAGE plpgsql;
`
	path := filepath.Join(project, "f.sql")
	if err := os.WriteFile(path, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}

	if !HasParserForExtensionIn(project, ".sql") {
		t.Fatal(".sql bound to antlr-postgresql must be discovered")
	}

	pf, err := NewCompositeParser(project, grammarOverridesFor(project)).Parse(path, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if pf.Parser != "antlr4" {
		t.Fatalf("parser = %q, want antlr4", pf.Parser)
	}

	var names []string
	for _, e := range pf.Entities["variables"] {
		names = append(names, e.Name)
	}
	if !containsString(names, "spliced_local") {
		t.Errorf("variables = %v, want the spliced PL/pgSQL local spliced_local", names)
	}
}
