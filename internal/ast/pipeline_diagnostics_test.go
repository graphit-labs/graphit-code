//go:build lancedb

package ast

import (
	"bytes"
	"context"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"gopkg.in/yaml.v3"
)

func TestASTProjectReloadDiagnostics(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	root := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	if err := os.WriteFile(filepath.Join(root, "example.unknown"), []byte("synthetic source"), 0600); err != nil {
		t.Fatal(err)
	}
	var console, project bytes.Buffer
	old := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&console, nil)))
	t.Cleanup(func() { slog.SetDefault(old) })
	logger := slog.New(slog.NewTextHandler(&project, nil))
	for _, invalidate := range []bool{false, true} {
		allEffectiveQueryFiles(root, logger)
		HasTreeSitterForExtensionIn(root, ".go", logger)
		if err := os.WriteFile(filepath.Join(root, ".graphit/ast/queries/broken.yaml"), []byte("language: ["+strings.Repeat(" ", project.Len()+1)), 0600); err != nil {
			t.Fatal(err)
		}
		v, _ := projectQueryStates.Load(root)
		state := v.(*queryDirState)
		state.mu.Lock()
		state.lastCheck = time.Time{}
		state.mu.Unlock()
		if invalidate {
			invalidateDerivedQueryCaches()
		}
		project.Reset()
		if _, err := collectFiles(root, logger); err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(project.String(), "YAML parse error") || console.Len() != 0 {
			t.Fatalf("reload routing invalidate=%v: project=%s console=%s", invalidate, project.String(), console.String())
		}
	}
}

func TestASTProjectParserDiagnostics(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	body, err := os.ReadFile("queries/go.yaml")
	if err != nil {
		t.Fatal(err)
	}
	var profile ExternalQueryFile
	if err := yaml.Unmarshal(body, &profile); err != nil {
		t.Fatal(err)
	}
	profile.Queries = append(profile.Queries, ExternalQueryDef{DataKey: "invalid", Pattern: "(graphit_missing_node) @name", GraphLabel: "Function"})
	profile.Embedded = []EmbeddedBlock{{Pattern: "(graphit_missing_embedded_node) @body", TextCapture: "body", Default: "sql"}}
	body, err = yaml.Marshal(profile)
	if err != nil {
		t.Fatal(err)
	}
	root := stageGrammarWithQueries(t, "go", "tree-sitter-go", ".go", "go.yaml", string(body))
	if err := os.WriteFile(filepath.Join(root, ".graphit/ast/queries/broken.yaml"), []byte("language: ["), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "example.go"), []byte("package fixture\nfunc Example() {}\n"), 0600); err != nil {
		t.Fatal(err)
	}
	var console, project bytes.Buffer
	old := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&console, nil)))
	t.Cleanup(func() { slog.SetDefault(old) })
	graphDir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: graphDir, IcebugDir: filepath.Join(graphDir, "graph.icebug")})
	defer func() { _ = db.Close() }()
	result, err := RunPipeline(context.Background(), db, root, PipelineOptions{Workers: 1, CacheDir: graphDir, Logger: slog.New(slog.NewTextHandler(&project, nil))})
	if err != nil || result.ParsedFiles != 1 {
		t.Fatalf("query warnings stopped parsing: %+v %v", result, err)
	}
	for _, marker := range []string{"YAML parse error", "skip resolved query: invalid pattern", "embedded pattern does not compile"} {
		if !strings.Contains(project.String(), marker) {
			t.Fatalf("missing %s in project log: %s", marker, project.String())
		}
	}
	if console.Len() != 0 {
		t.Fatalf("parser warning leaked: %s", console.String())
	}
}

func TestASTProjectWriteDiagnostics(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	root := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	if err := os.WriteFile(filepath.Join(root, "example.go"), []byte("package fixture\nfunc Example() {}\n"), 0600); err != nil {
		t.Fatal(err)
	}
	graphDir := t.TempDir()
	blocked := filepath.Join(graphDir, "blocked")
	if err := os.WriteFile(blocked, []byte("synthetic write blocker"), 0600); err != nil {
		t.Fatal(err)
	}
	db := NewLadybugDB(LadybugConfig{StoreDir: graphDir, IcebugDir: filepath.Join(blocked, "graph.icebug")})
	defer func() { _ = db.Close() }()
	var console, project bytes.Buffer
	old := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&console, nil)))
	t.Cleanup(func() { slog.SetDefault(old) })
	result, err := RunPipeline(context.Background(), db, root, PipelineOptions{Workers: 1, CacheDir: graphDir, Logger: slog.New(slog.NewTextHandler(&project, nil))})
	if err != nil {
		t.Fatal(err)
	}
	if result.WriteErrorCount != 1 || len(result.WriteErrorFiles) != 1 {
		t.Fatalf("missing write failure: %+v", result)
	}
	if !strings.Contains(project.String(), "AST graph rebuild failed") || !strings.Contains(project.String(), "not a directory") {
		t.Fatalf("missing write details: %s", project.String())
	}
	if console.Len() != 0 {
		t.Fatalf("write failure leaked: %s", console.String())
	}
}
