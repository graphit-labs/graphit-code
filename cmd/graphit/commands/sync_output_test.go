package commands

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/output"
)

func TestSyncOutputRejectsGlobalVerboseFlags(t *testing.T) {
	for _, command := range [][]string{{"init"}, {"sync"}, {"ast", "index"}, {"knowledge", "index"}, {"memory", "index"}} {
		for _, flag := range []string{"--verbose", "-v"} {
			args := append(append([]string(nil), command...), flag)
			_, err := executeCommand(args...)
			if err == nil || (!strings.Contains(err.Error(), "unknown flag") && !strings.Contains(err.Error(), "unknown shorthand flag")) {
				t.Errorf("%v: expected unknown flag, got %v", args, err)
			}
		}
	}
}

func TestSyncOutputVerboseBelongsOnlyToLive(t *testing.T) {
	if rootCmd.PersistentFlags().Lookup("verbose") != nil {
		t.Fatal("verbose must not be a global flag")
	}
	cmd := newLiveCmd()
	for _, flag := range []string{"--verbose", "-v"} {
		if err := cmd.ParseFlags([]string{flag}); err != nil {
			t.Fatal(err)
		}
		verbose, err := cmd.Flags().GetBool("verbose")
		if err != nil || !verbose {
			t.Fatalf("live %s: verbose=%v err=%v", flag, verbose, err)
		}
	}
}

func TestSyncOutputKeepsSummaryAndWarningsWithoutDiagnosticBreakdowns(t *testing.T) {
	result := &ast.PipelineResult{
		TotalFiles: 3, ParsedFiles: 3, DiscoverTime: time.Millisecond,
		WriteTime: time.Second, TimeoutCount: 1, ErrorCount: 1,
		WriteErrorCount: 1, EmptyCount: 1, EmptyFiles: []string{"empty.go"},
		EngineStats: map[string]int{"tree-sitter": 3},
	}
	var out bytes.Buffer
	printASTIndexResult(output.NewPrinterTo("", &out), result)
	for _, want := range []string{"Completed with 2 error(s)", "Timeouts: 1", "Parse errors: 1", "Write errors: 1"} {
		if !strings.Contains(out.String(), want) {
			t.Errorf("lost %q: %s", want, out.String())
		}
	}
	for _, detail := range []string{"Timing:", "Write breakdown:", "empty.go", "tree-sitter"} {
		if strings.Contains(out.String(), detail) {
			t.Errorf("unexpected detail %q: %s", detail, out.String())
		}
	}
}
