package plsql

import (
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

func probeRSSMB() int64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "VmRSS:") {
			var kb int64
			_, _ = fmt.Sscanf(strings.TrimSpace(strings.TrimPrefix(line, "VmRSS:")), "%d", &kb)
			return kb / 1024
		}
	}
	return 0
}

// TestStageProbe parses one file with ONLY the SLL stage or ONLY the LL stage, to
// determine which prediction mode is responsible for the unbounded memory growth
// observed on some real Oracle PL/SQL files.
//
//	GRAPHIT_STAGE_FILE=/path/f.sql GRAPHIT_STAGE=SLL|LL \
//	  go test ./internal/ast/antlr/plsql/ -run TestStageProbe -v -count=1
func TestStageProbe(t *testing.T) {
	path := os.Getenv("GRAPHIT_STAGE_FILE")
	if path == "" {
		t.Skip("set GRAPHIT_STAGE_FILE (and GRAPHIT_STAGE=SLL|LL) to probe a single file")
	}
	src, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	mode := antlrcommon.ModeSLL
	name := "SLL"
	if os.Getenv("GRAPHIT_STAGE") == "LL" {
		mode = antlrcommon.ModeLL
		name = "LL"
	}

	fmt.Fprintf(os.Stderr, "STAGE=%s file=%s size=%dB rss_before=%dMB\n", name, path, len(src), probeRSSMB())

	preprocessed := Preprocess(string(src))
	input := antlr.NewInputStream(preprocessed)
	lexer := NewPlSqlLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
	p := NewPlSqlParser(tokens)
	p.RemoveErrorListeners()
	antlrcommon.ConfigureParser(p, tokens, &p.BuildParseTrees, mode)

	t0 := time.Now()
	var tree antlr.ParseTree
	func() {
		defer func() {
			if r := recover(); r != nil {
				fmt.Fprintf(os.Stderr, "STAGE=%s PANIC/BAIL after %v rss=%dMB\n", name, time.Since(t0), probeRSSMB())
			}
		}()
		tree = p.Sql_script()
	}()
	fmt.Fprintf(os.Stderr, "STAGE=%s done in %v tree=%v rss_after=%dMB\n",
		name, time.Since(t0), tree != nil, probeRSSMB())
}
