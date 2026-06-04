package ast

import (
	"os"
	"path/filepath"
	"sync"
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

func TestProjectQueriesDir(t *testing.T) {
	dir := projectQueriesDir("/home/user/project")
	expected := filepath.Join("/home/user/project", ".graphit", "ast", "queries")
	if dir != expected {
		t.Errorf("expected %q, got %q", expected, dir)
	}
}

// TestLoadQueriesFromEmbed was removed: loadQueriesFromEmbed no longer exists.
// AST YAML files are now embedded in the launcher and extracted to the runtime
// directory, not compiled into the core binary.

// TestEnsureDefaultQueries was removed: EnsureDefaultQueries no longer exists.
// The launcher handles extracting default queries to the runtime directory.

func TestResolveQueriesForLang_ProjectOverridesGlobal(t *testing.T) {
	// Clear caches
	externalQueryCache = sync.Map{}
	mergedQueryCache = sync.Map{}

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

// TestResolveQueriesForLang_FallsBackToEmbedded was removed: the embedded
// fallback no longer exists in the core binary. Queries are loaded from the
// runtime directory (extracted by the launcher) instead.

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
