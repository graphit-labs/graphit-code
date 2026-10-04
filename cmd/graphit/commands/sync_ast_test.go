package commands

import (
	"bytes"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func TestSyncASTPipelineOptions(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	var console bytes.Buffer
	old := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&console, nil)))
	t.Cleanup(func() { slog.SetDefault(old) })
	root := t.TempDir()
	logger, closeLog, err := slogutil.ProjectLogger(root, "ast")
	if err != nil {
		t.Fatal(err)
	}
	opts := syncASTPipelineOptions(nil, output.NewPrinter(""), logger)
	if opts.Logger != logger || opts.OnProgress == nil || opts.Workers == 0 || opts.CacheDir != ast.DefaultLadybugConfig().StoreDir {
		t.Fatalf("sync lost diagnostics/progress or AST defaults: %+v", opts)
	}
	opts.Logger.Warn("SCIP unavailable; using syntax parser", "error", "synthetic private stack marker")
	closeLog()
	data, err := os.ReadFile(filepath.Join(root, ".graphit/runtime/daemon/daemon.log"))
	if err != nil || !strings.Contains(string(data), "synthetic private stack marker") {
		t.Fatalf("project diagnostic missing: %s, %v", data, err)
	}
	if console.Len() != 0 {
		t.Fatalf("diagnostic leaked to default/CLI logger: %s", console.String())
	}
}
