//go:build lancedb

package commands

import (
	"bytes"
	"context"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func TestSyncASTProjectDiagnostics(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Setenv("GRAPHIT_AST_PARSE_TIMEOUT", "1ns")
	root := t.TempDir()
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	profile, err := os.ReadFile("../../../internal/ast/queries/go.yaml")
	if err != nil {
		t.Fatal(err)
	}
	queryDir := filepath.Join(root, ".graphit/ast/queries")
	if err := os.MkdirAll(queryDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(queryDir, "go.yaml"), profile, 0600); err != nil {
		t.Fatal(err)
	}
	code := "package fixture\n" + strings.Repeat("func Example() {}\n", 10000)
	if err := os.WriteFile(filepath.Join(root, "example.go"), []byte(code), 0600); err != nil {
		t.Fatal(err)
	}
	var console bytes.Buffer
	old := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&console, nil)))
	t.Cleanup(func() { slog.SetDefault(old) })
	logger, closeLog, err := slogutil.ProjectLogger(root, "ast")
	if err != nil {
		t.Fatal(err)
	}
	defer closeLog()
	opts := syncASTPipelineOptions(nil, output.NewPrinter(""), logger)
	opts.CacheDir = t.TempDir()
	opts.Workers = 1
	var processed, failures int
	opts.OnProgress = func(phase string, current, total, errors int) {
		if phase == "parsing" {
			processed, failures = current, errors
		}
	}
	cfg := ast.DefaultLadybugConfig()
	cfg.StoreDir = opts.CacheDir
	cfg.IcebugDir = filepath.Join(opts.CacheDir, "graph.icebug")
	db := ast.NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()
	result, err := ast.RunPipeline(context.Background(), db, root, opts)
	if err != nil {
		t.Fatal(err)
	}
	if result.ErrorCount != 1 || result.TimeoutCount != 1 || processed != 1 || failures != 1 {
		t.Fatalf("lost timeout summary/progress: %+v processed=%d errors=%d", result, processed, failures)
	}
	data, err := os.ReadFile(filepath.Join(root, ".graphit/runtime/daemon/daemon.log"))
	if err != nil || !strings.Contains(string(data), "AST parse failed") || !strings.Contains(string(data), "context deadline exceeded") {
		t.Fatalf("missing project parse diagnostic: %s, %v", data, err)
	}
	if console.Len() != 0 {
		t.Fatalf("pipeline leaked detailed output: %s", console.String())
	}
}
