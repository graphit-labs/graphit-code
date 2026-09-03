package ast

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

func writeLockfileConfig(t *testing.T, projectDir string, cfg map[string]any) {
	t.Helper()
	data, err := json.Marshal(map[string]any{"config": cfg})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func stageQueryFile(t *testing.T, dir, name, body string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestProjectQueriesDirFollowsConfig(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	project := t.TempDir()
	writeLockfileConfig(t, project, map[string]any{
		"ast": map[string]any{"queries_dir": "tooling/grammars"},
	})

	got := projectQueriesDir(project)
	want := filepath.Join(project, "tooling", "grammars")
	if got != want {
		t.Errorf("queries dir = %q, want %q", got, want)
	}
}

// The configured directory REPLACES the default one rather than adding to it:
// a project has one grammar directory, and two would mean two answers for the
// same language with no rule to choose between them.
func TestConfiguredQueriesDirReplacesTheBrandDir(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	project := t.TempDir()
	writeLockfileConfig(t, project, map[string]any{
		"ast": map[string]any{"queries_dir": "grammars"},
	})

	const tracked = `language: tracked_lang
extensions: [".tracked"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	const ignored = `language: ignored_lang
extensions: [".ignored"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	stageQueryFile(t, filepath.Join(project, "grammars"), "tracked.yaml", tracked)
	stageQueryFile(t, filepath.Join(project, brand.DotDir(), "ast", "queries"), "ignored.yaml", ignored)

	files, err := LoadExternalQueries(project)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(files) != 1 {
		t.Fatalf("expected only the configured directory to be read, got %d files", len(files))
	}
	if files[0].Language != "tracked_lang" {
		t.Errorf("read %q, want the file in the configured directory", files[0].Language)
	}
}

// Changing ast.queries_dir under a running process has to land without a restart:
// the daemon lives for days, and the signature the loader compares folds the
// directory path in for exactly this reason.
func TestQueriesDirChangeIsPickedUp(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	project := t.TempDir()
	const first = `language: first_lang
extensions: [".first"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	const second = `language: second_lang
extensions: [".second"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	stageQueryFile(t, filepath.Join(project, "a"), "first.yaml", first)
	stageQueryFile(t, filepath.Join(project, "b"), "second.yaml", second)

	writeLockfileConfig(t, project, map[string]any{"ast": map[string]any{"queries_dir": "a"}})
	if got := langNamesOf(loadProjectCached(project)); len(got) != 1 || got[0] != "first_lang" {
		t.Fatalf("with queries_dir=a the project declares %v, want [first_lang]", got)
	}

	writeLockfileConfig(t, project, map[string]any{"ast": map[string]any{"queries_dir": "b"}})
	InvalidateQueryCaches()
	if got := langNamesOf(loadProjectCached(project)); len(got) != 1 || got[0] != "second_lang" {
		t.Errorf("after moving queries_dir to b the project declares %v, want [second_lang]", got)
	}
}

func langNamesOf(files []ExternalQueryFile) []string {
	names := make([]string, 0, len(files))
	for _, f := range files {
		names = append(names, f.Language)
	}
	return names
}

func stageRuntimeLevel(t *testing.T, name, body string) {
	t.Helper()
	if home, _ := os.UserHomeDir(); home == "" || !isTempPath(home) {
		t.Fatalf("refusing to write to the real runtime directory (HOME=%q)", home)
	}
	stageQueryFile(t, filepath.Join(brand.RuntimeDir(version.Version), "ast", "queries"), name, body)
}

func isTempPath(p string) bool {
	rel, err := filepath.Rel(os.TempDir(), p)
	return err == nil && rel != ".." && !filepath.IsAbs(rel) && rel[0] != '.'
}

const runtimeBaseYAML = `language: baselang
grammar: tree-sitter-go
extensions: [".baselang"]
declaration_types: ["function_declaration"]
comment_types: ["comment"]
context_types:
  function_declaration: function
  type_declaration: class
complexity:
  node_types: ["if_statement"]
  operators: ["&&"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
  - data_key: types
    graph_label: Type
    pattern: '(type_declaration (type_spec name: (type_identifier) @name))'
`

func TestMergeTrueMergesOntoTheLevelBelow(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	stageRuntimeLevel(t, "baselang.yaml", runtimeBaseYAML)

	project := t.TempDir()
	const partial = `language: baselang
merge: true
context_types:
  method_declaration: method
complexity:
  node_types: ["if_statement", "for_statement"]
queries:
  - data_key: types
    graph_label: Struct
    pattern: '(type_declaration (type_spec name: (type_identifier) @name))'
  - data_key: calls
    graph_label: Function
    type: relation
    relation_type: CALLS
    pattern: '(call_expression function: (identifier) @name)'
`
	stageQueryFile(t, projectQueriesDir(project), "baselang.yaml", partial)

	resolved := resolveQueriesForLang(project, "baselang", ".baselang")
	if len(resolved) != 1 {
		t.Fatalf("expected 1 resolved file, got %d", len(resolved))
	}
	merged := resolved[0]

	if got := merged.Grammar; got != "tree-sitter-go" {
		t.Errorf("grammar = %q, want the runtime's tree-sitter-go", got)
	}
	if len(merged.Extensions) != 1 || merged.Extensions[0] != ".baselang" {
		t.Errorf("extensions = %v, want the runtime's [.baselang]", merged.Extensions)
	}

	if len(merged.DeclarationTypes) != 1 || merged.DeclarationTypes[0] != "function_declaration" {
		t.Errorf("declaration_types = %v, want the runtime's", merged.DeclarationTypes)
	}
	if len(merged.CommentTypes) != 1 {
		t.Errorf("comment_types = %v, want the runtime's", merged.CommentTypes)
	}

	for _, key := range []string{"function_declaration", "type_declaration", "method_declaration"} {
		if _, ok := merged.ContextTypes[key]; !ok {
			t.Errorf("context_types lost %q: %v", key, merged.ContextTypes)
		}
	}

	if merged.Complexity == nil {
		t.Fatal("complexity was dropped")
	}
	if len(merged.Complexity.NodeTypes) != 2 {
		t.Errorf("complexity.node_types = %v, want the project's two", merged.Complexity.NodeTypes)
	}
	if len(merged.Complexity.Operators) != 1 {
		t.Errorf("complexity.operators = %v, want the runtime's", merged.Complexity.Operators)
	}

	byKey := map[string]ExternalQueryDef{}
	for _, q := range merged.Queries {
		byKey[q.DataKey] = q
	}
	if len(merged.Queries) != 3 {
		t.Fatalf("queries = %d, want 3 (functions inherited, types replaced, calls added)", len(merged.Queries))
	}
	if _, ok := byKey["functions"]; !ok {
		t.Error("the inherited `functions` query was dropped")
	}
	if got := byKey["types"].GraphLabel; got != "Struct" {
		t.Errorf("types graph_label = %q, want the project's Struct", got)
	}
	if got := byKey["calls"].RelationType; got != "CALLS" {
		t.Errorf("calls relation_type = %q, want CALLS", got)
	}
}

// Without the declaration nothing changes: the winning level is the whole
// language, as it has always been.
func TestWithoutMergeTheProjectFileStillReplaces(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	stageRuntimeLevel(t, "baselang.yaml", runtimeBaseYAML)

	project := t.TempDir()
	const full = `language: baselang
extensions: [".baselang"]
queries:
  - data_key: types
    graph_label: Struct
    pattern: '(type_declaration (type_spec name: (type_identifier) @name))'
`
	stageQueryFile(t, projectQueriesDir(project), "baselang.yaml", full)

	resolved := resolveQueriesForLang(project, "baselang", ".baselang")
	if len(resolved) != 1 {
		t.Fatalf("expected 1 resolved file, got %d", len(resolved))
	}
	if len(resolved[0].Queries) != 1 {
		t.Errorf("queries = %d, want only the project's one", len(resolved[0].Queries))
	}
	if len(resolved[0].DeclarationTypes) != 0 {
		t.Errorf("declaration_types = %v, want none: replacement drops what it does not restate",
			resolved[0].DeclarationTypes)
	}
}

// "for all levels": the user's own directory merges onto the runtime the same
// way, and a project merging onto a merged user level gets both.
func TestMergeAppliesAtEveryLevel(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	stageRuntimeLevel(t, "baselang.yaml", runtimeBaseYAML)

	const userLevel = `language: baselang
merge: true
queries:
  - data_key: user_added
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	stageQueryFile(t, userQueriesDir(), "baselang.yaml", userLevel)

	project := t.TempDir()
	const projectLevel = `language: baselang
merge: true
queries:
  - data_key: project_added
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	stageQueryFile(t, projectQueriesDir(project), "baselang.yaml", projectLevel)

	resolved := resolveQueriesForLang(project, "baselang", ".baselang")
	if len(resolved) != 1 {
		t.Fatalf("expected 1 resolved file, got %d", len(resolved))
	}
	seen := map[string]bool{}
	for _, q := range resolved[0].Queries {
		seen[q.DataKey] = true
	}
	for _, key := range []string{"functions", "types", "user_added", "project_added"} {
		if !seen[key] {
			t.Errorf("the three-level fold lost %q: have %v", key, seen)
		}
	}
	if resolved[0].Grammar != "tree-sitter-go" {
		t.Errorf("grammar = %q, want the runtime's, carried through two folds", resolved[0].Grammar)
	}
}

// A merging file must reach the extension tables as the merged one, or a project
// that only adds a query unregisters the extension it never mentioned.
func TestMergedProjectFileKeepsItsExtensionRegistered(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	if lang, err := resolveTreeSitterLang("go", "tree-sitter-go"); err != nil || lang == nil {
		t.Skipf("go grammar unavailable: %v", err)
	}
	stageRuntimeLevel(t, "baselang.yaml", runtimeBaseYAML)

	project := t.TempDir()
	const partial = `language: baselang
merge: true
queries:
  - data_key: calls
    graph_label: Function
    type: relation
    relation_type: CALLS
    pattern: '(call_expression function: (identifier) @name)'
`
	stageQueryFile(t, projectQueriesDir(project), "baselang.yaml", partial)
	InvalidateQueryCaches()

	cfg, ok := tsLangConfigFor(project, ".baselang")
	if !ok {
		t.Fatal(".baselang is not parseable in the project that merged onto its grammar")
	}
	if cfg.Grammar != "tree-sitter-go" {
		t.Errorf("grammar for .baselang = %q, want the inherited tree-sitter-go", cfg.Grammar)
	}
}

// A file that declares a language no lower level knows has nothing to merge onto,
// and must still work as its own declaration.
func TestMergeWithNoBaseIsJustTheFile(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)

	project := t.TempDir()
	const standalone = `language: nobase
grammar: tree-sitter-go
extensions: [".nobase"]
merge: true
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	stageQueryFile(t, projectQueriesDir(project), "nobase.yaml", standalone)

	resolved := resolveQueriesForLang(project, "nobase", ".nobase")
	if len(resolved) != 1 {
		t.Fatalf("expected 1 resolved file, got %d", len(resolved))
	}
	if len(resolved[0].Queries) != 1 || resolved[0].Grammar != "tree-sitter-go" {
		t.Errorf("standalone merging file came back wrong: %+v", resolved[0])
	}
}

// mergeOnto returns the upper level, never the union: a language only the lower
// level declares must not surface as a project-level file, or resolveQueriesForLang
// would stop falling through and every project would answer for every language.
func TestMergeOntoReturnsOnlyTheUpperLevel(t *testing.T) {
	base := []ExternalQueryFile{
		{Language: "kept", Extensions: []string{".kept"}},
		{Language: "shared", Extensions: []string{".shared"}, Grammar: "tree-sitter-shared"},
	}
	over := []ExternalQueryFile{
		{Language: "shared", Merge: true},
	}

	merged := mergeOnto(base, over)
	if len(merged) != 1 {
		t.Fatalf("merged level has %d files, want only the upper level's 1", len(merged))
	}
	if merged[0].Grammar != "tree-sitter-shared" {
		t.Errorf("grammar = %q, want the inherited one", merged[0].Grammar)
	}
	if base[1].Merge {
		t.Error("mergeOnto mutated the level below it")
	}
	if over[0].Grammar != "" {
		t.Error("mergeOnto mutated the level it was given")
	}
}
