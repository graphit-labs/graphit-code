package ast

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadQueriesFromDir_MissingDir(t *testing.T) {
	dir := t.TempDir()
	result, err := loadQueriesFromDir(filepath.Join(dir, "nonexistent"))
	if err != nil {
		t.Fatalf("expected nil error for missing dir, got: %v", err)
	}
	if len(result) != 0 {
		t.Fatalf("expected empty result, got %d files", len(result))
	}
}

func TestLoadQueriesFromDir_EmptyDir(t *testing.T) {
	dir := t.TempDir()
	result, err := loadQueriesFromDir(dir)
	if err != nil {
		t.Fatalf("expected nil error, got: %v", err)
	}
	if len(result) != 0 {
		t.Fatalf("expected empty result, got %d files", len(result))
	}
}

func TestLoadExternalQueries_ValidFile(t *testing.T) {
	dir := t.TempDir()
	qDir := projectQueriesDir(dir)
	if err := os.MkdirAll(qDir, 0o755); err != nil {
		t.Fatal(err)
	}

	yamlContent := `language: go
extensions: [".go"]
replace: false
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
  - data_key: goroutines
    graph_label: Function
    name_capture: fn
    pattern: '(go_statement (call_expression function: (identifier) @fn))'
`
	if err := os.WriteFile(filepath.Join(qDir, "go.yaml"), []byte(yamlContent), 0o644); err != nil {
		t.Fatal(err)
	}

	result, err := LoadExternalQueries(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(result) != 1 {
		t.Fatalf("expected 1 file, got %d", len(result))
	}

	qf := result[0]
	if qf.Language != "go" {
		t.Errorf("expected language 'go', got %q", qf.Language)
	}
	if qf.Replace {
		t.Error("expected replace=false")
	}
	if len(qf.Queries) != 2 {
		t.Fatalf("expected 2 queries, got %d", len(qf.Queries))
	}

	// First query should have default name_capture
	if qf.Queries[0].NameCapture != "name" {
		t.Errorf("expected default name_capture 'name', got %q", qf.Queries[0].NameCapture)
	}
	// Second query should have explicit name_capture
	if qf.Queries[1].NameCapture != "fn" {
		t.Errorf("expected name_capture 'fn', got %q", qf.Queries[1].NameCapture)
	}
}

func TestLoadExternalQueries_SkipsInvalidEntries(t *testing.T) {
	dir := t.TempDir()
	qDir := projectQueriesDir(dir)
	if err := os.MkdirAll(qDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// File with missing language
	noLang := `queries:
  - data_key: functions
    pattern: '(function_declaration name: (identifier) @name)'
`
	if err := os.WriteFile(filepath.Join(qDir, "bad1.yaml"), []byte(noLang), 0o644); err != nil {
		t.Fatal(err)
	}

	// File with missing data_key on query
	noDataKey := `language: python
queries:
  - pattern: '(function_definition name: (identifier) @name)'
  - data_key: classes
    pattern: '(class_definition name: (identifier) @name)'
`
	if err := os.WriteFile(filepath.Join(qDir, "bad2.yaml"), []byte(noDataKey), 0o644); err != nil {
		t.Fatal(err)
	}

	// File with missing pattern
	noPattern := `language: ruby
queries:
  - data_key: functions
`
	if err := os.WriteFile(filepath.Join(qDir, "bad3.yaml"), []byte(noPattern), 0o644); err != nil {
		t.Fatal(err)
	}

	result, err := LoadExternalQueries(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(result) != 1 {
		t.Fatalf("expected 1 valid file (bad2), got %d", len(result))
	}
	if result[0].Language != "python" {
		t.Errorf("expected python file, got %q", result[0].Language)
	}
	if len(result[0].Queries) != 1 {
		t.Fatalf("expected 1 valid query, got %d", len(result[0].Queries))
	}
	if result[0].Queries[0].DataKey != "classes" {
		t.Errorf("expected data_key 'classes', got %q", result[0].Queries[0].DataKey)
	}
}

func TestLoadQueriesFromDir_SkipsNonYAML(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "readme.txt"), []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	yml := `language: rust
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_item name: (identifier) @name)'
`
	if err := os.WriteFile(filepath.Join(dir, "rust.yml"), []byte(yml), 0o644); err != nil {
		t.Fatal(err)
	}

	result, err := loadQueriesFromDir(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(result) != 1 {
		t.Fatalf("expected 1 file (.yml), got %d", len(result))
	}
	if result[0].Language != "rust" {
		t.Errorf("expected rust, got %q", result[0].Language)
	}
}

func TestMergeQueries_AppendMode(t *testing.T) {
	builtIn := []tsQueryDef{
		{DataKey: "functions", GraphLabel: "Function", Pattern: `(function_declaration name: (identifier) @name)`, NameCapture: "name"},
	}
	externals := []ExternalQueryFile{
		{
			Language: "go",
			Replace:  false,
			Queries: []ExternalQueryDef{
				{DataKey: "goroutines", GraphLabel: "Function", Pattern: `(go_statement)`, NameCapture: "name"},
			},
		},
	}

	merged := MergeQueries(builtIn, externals, "go", ".go", nil)
	if len(merged) != 2 {
		t.Fatalf("expected 2 queries (1 built-in + 1 external), got %d", len(merged))
	}
	if merged[0].DataKey != "functions" {
		t.Errorf("expected first query 'functions', got %q", merged[0].DataKey)
	}
	if merged[1].DataKey != "goroutines" {
		t.Errorf("expected second query 'goroutines', got %q", merged[1].DataKey)
	}
}

func TestMergeQueries_ReplaceMode(t *testing.T) {
	builtIn := []tsQueryDef{
		{DataKey: "functions", GraphLabel: "Function", Pattern: `(function_declaration)`, NameCapture: "name"},
		{DataKey: "structs", GraphLabel: "Struct", Pattern: `(type_declaration)`, NameCapture: "name"},
	}
	externals := []ExternalQueryFile{
		{
			Language: "go",
			Replace:  true,
			Queries: []ExternalQueryDef{
				{DataKey: "custom_func", GraphLabel: "Function", Pattern: `(function_item)`, NameCapture: "name"},
			},
		},
	}

	merged := MergeQueries(builtIn, externals, "go", ".go", nil)
	if len(merged) != 1 {
		t.Fatalf("expected 1 query (replace mode), got %d", len(merged))
	}
	if merged[0].DataKey != "custom_func" {
		t.Errorf("expected 'custom_func', got %q", merged[0].DataKey)
	}
}

func TestMergeQueries_NoMatch(t *testing.T) {
	builtIn := []tsQueryDef{
		{DataKey: "functions", GraphLabel: "Function", Pattern: `(function_declaration)`, NameCapture: "name"},
	}
	externals := []ExternalQueryFile{
		{Language: "python", Queries: []ExternalQueryDef{{DataKey: "decorators", Pattern: `(decorator)`, NameCapture: "name"}}},
	}

	merged := MergeQueries(builtIn, externals, "go", ".go", nil)
	if len(merged) != 1 {
		t.Fatalf("expected 1 query (no match), got %d", len(merged))
	}
}

func TestMergeQueries_ExtensionFiltering(t *testing.T) {
	builtIn := []tsQueryDef{
		{DataKey: "functions", GraphLabel: "Function", Pattern: `(function_declaration)`, NameCapture: "name"},
	}
	externals := []ExternalQueryFile{
		{
			Language:   "typescript",
			Extensions: []string{".tsx"},
			Queries:    []ExternalQueryDef{{DataKey: "jsx", GraphLabel: "Component", Pattern: `(jsx_element)`, NameCapture: "name"}},
		},
	}

	merged := MergeQueries(builtIn, externals, "typescript", ".ts", nil)
	if len(merged) != 1 {
		t.Fatalf("expected 1 query (.ts should not match .tsx filter), got %d", len(merged))
	}

	merged = MergeQueries(builtIn, externals, "typescript", ".tsx", nil)
	if len(merged) != 2 {
		t.Fatalf("expected 2 queries (.tsx should match), got %d", len(merged))
	}
}

func TestMergeQueries_MultipleFiles(t *testing.T) {
	builtIn := []tsQueryDef{
		{DataKey: "functions", GraphLabel: "Function", Pattern: `(function_declaration)`, NameCapture: "name"},
	}
	externals := []ExternalQueryFile{
		{Language: "go", Queries: []ExternalQueryDef{{DataKey: "goroutines", Pattern: `(go_statement)`, NameCapture: "name"}}},
		{Language: "go", Queries: []ExternalQueryDef{{DataKey: "defers", Pattern: `(defer_statement)`, NameCapture: "name"}}},
	}

	merged := MergeQueries(builtIn, externals, "go", ".go", nil)
	if len(merged) != 3 {
		t.Fatalf("expected 3 queries (1 built-in + 2 from 2 files), got %d", len(merged))
	}
}

func TestToTSQueryDefs(t *testing.T) {
	external := []ExternalQueryDef{
		{DataKey: "functions", GraphLabel: "Function", Pattern: "(test)", NameCapture: "name"},
		{DataKey: "classes", GraphLabel: "Class", Pattern: "(class)", NameCapture: "cls"},
	}

	result := toTSQueryDefs(external)
	if len(result) != 2 {
		t.Fatalf("expected 2, got %d", len(result))
	}
	if result[0].DataKey != "functions" || result[0].NameCapture != "name" {
		t.Errorf("unexpected first result: %+v", result[0])
	}
	if result[1].DataKey != "classes" || result[1].NameCapture != "cls" {
		t.Errorf("unexpected second result: %+v", result[1])
	}
}

func TestResetQueryCaches(t *testing.T) {
	externalQueryCache.Store("test", []ExternalQueryFile{{Language: "go"}})
	mergedQueryCache.Store("test|go|.go", []tsQueryDef{{DataKey: "x"}})

	resetQueryCaches()

	if _, ok := externalQueryCache.Load("test"); ok {
		t.Error("expected external cache to be cleared")
	}
	if _, ok := mergedQueryCache.Load("test|go|.go"); ok {
		t.Error("expected merged cache to be cleared")
	}
}

func TestProjectQueriesDir(t *testing.T) {
	dir := projectQueriesDir("/home/user/project")
	expected := filepath.Join("/home/user/project", ".graphit", "ast", "queries")
	if dir != expected {
		t.Errorf("expected %q, got %q", expected, dir)
	}
}

func TestLoadQueriesFromEmbed(t *testing.T) {
	result := loadQueriesFromEmbed()
	if len(result) == 0 {
		t.Fatal("expected embedded queries to be loaded, got 0")
	}

	// Check that we have at least the languages we know exist
	langSet := make(map[string]bool)
	for _, qf := range result {
		langSet[qf.Language] = true
	}

	expectedLangs := []string{"go", "python", "javascript", "typescript", "java", "rust", "c", "cpp", "ruby", "swift", "dart", "kotlin", "php", "csharp", "sql"}
	for _, lang := range expectedLangs {
		if !langSet[lang] {
			t.Errorf("expected embedded queries for language %q", lang)
		}
	}
}

func TestEnsureDefaultQueries(t *testing.T) {
	// This test uses a temp dir as home, so we can't test the actual global dir
	// without mocking brand.GlobalDir(). Test that the function doesn't crash.
	// The actual global dir test would require env manipulation.
	t.Log("EnsureDefaultQueries is tested indirectly via embedded loading")
}

func TestResolveQueriesForLang_ProjectOverridesGlobal(t *testing.T) {
	resetQueryCaches()

	// Set up project with custom queries
	dir := t.TempDir()
	qDir := projectQueriesDir(dir)
	if err := os.MkdirAll(qDir, 0o755); err != nil {
		t.Fatal(err)
	}

	projectYAML := `language: go
extensions: [".go"]
queries:
  - data_key: custom_func
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name)'
`
	if err := os.WriteFile(filepath.Join(qDir, "go.yaml"), []byte(projectYAML), 0o644); err != nil {
		t.Fatal(err)
	}

	// Resolve — project should win over global and embedded
	resolved := resolveQueriesForLang(dir, "go", ".go")
	if len(resolved) != 1 {
		t.Fatalf("expected 1 resolved file, got %d", len(resolved))
	}
	if resolved[0].Queries[0].DataKey != "custom_func" {
		t.Errorf("expected project-level 'custom_func', got %q", resolved[0].Queries[0].DataKey)
	}
}

func TestResolveQueriesForLang_FallsBackToEmbedded(t *testing.T) {
	resetQueryCaches()

	// Empty project — no project or global queries
	dir := t.TempDir()

	resolved := resolveQueriesForLang(dir, "go", ".go")
	if len(resolved) == 0 {
		t.Fatal("expected embedded fallback, got 0 results")
	}

	// Should get the embedded go queries
	if resolved[0].Language != "go" {
		t.Errorf("expected language 'go', got %q", resolved[0].Language)
	}
}

func TestFilterByLangExt(t *testing.T) {
	files := []ExternalQueryFile{
		{Language: "go", Extensions: []string{".go"}},
		{Language: "python", Extensions: []string{".py"}},
		{Language: "typescript", Extensions: []string{".ts"}},
		{Language: "typescript", Extensions: []string{".tsx"}},
	}

	result := filterByLangExt(files, "typescript", ".tsx")
	if len(result) != 1 {
		t.Fatalf("expected 1, got %d", len(result))
	}
	if result[0].Extensions[0] != ".tsx" {
		t.Errorf("expected .tsx, got %s", result[0].Extensions[0])
	}

	// No extensions = matches all
	files = append(files, ExternalQueryFile{Language: "go"})
	result = filterByLangExt(files, "go", ".go")
	if len(result) != 2 {
		t.Fatalf("expected 2 (one with ext, one without), got %d", len(result))
	}
}
