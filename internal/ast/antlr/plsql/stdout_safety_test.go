package plsql

import (
	"bytes"
	"io"
	"os"
	"testing"

	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// captureStdout runs fn while os.Stdout is redirected to a pipe and returns
// everything fn wrote to stdout.
func captureStdout(fn func()) string {
	old := os.Stdout
	r, w, _ := os.Pipe()
	os.Stdout = w
	fn()
	_ = w.Close()
	os.Stdout = old
	var buf bytes.Buffer
	_, _ = io.Copy(&buf, r)
	return buf.String()
}

// TestSilentErrorStrategyNeverWritesStdout deterministically exercises the
// buggy path: a RecognitionException whose concrete type is none of
// *NoViableAltException / *InputMisMatchException / *FailedPredicateException
// hits DefaultErrorStrategy.ReportError's default case, which does
// fmt.Println("unknown recognition error type: ...") to stdout. SilentErrorStrategy
// must NOT let that reach stdout (it corrupts the MCP JSON-RPC stream).
func TestSilentErrorStrategyNeverWritesStdout(t *testing.T) {
	input := antlr.NewInputStream("SELECT 1 FROM dual")
	lexer := NewPlSqlLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)
	p := NewPlSqlParser(tokens)
	p.RemoveErrorListeners()

	strat := antlrcommon.NewSilentErrorStrategy()
	e := antlr.NewBaseRecognitionException("synthetic", p, tokens, nil)

	got := captureStdout(func() { strat.ReportError(p, e) })
	if got != "" {
		t.Errorf("SilentErrorStrategy.ReportError wrote to stdout: %q", got)
	}
}

// TestStdoutPollutionRepro asserts that parsing malformed PL/SQL never writes
// to stdout. The process runs as a stdio MCP server, so any stray write to
// stdout corrupts the JSON-RPC framing. This is a Red test for the
// "unknown recognition error type" leak from antlr's DefaultErrorStrategy.
func TestStdoutPollutionRepro(t *testing.T) {
	inputs := []string{
		"CREATE TABLE ;;; garbage )))) (((( SELECT FROM WHERE;",
		"BEGIN @@@ !!! ??? END;",
		"SELECT * FROM WHERE GROUP BY HAVING ) ) ) ;",
		"}{][)(*&^%$#@!",
		"DECLARE x NUMBER := ; BEGIN NULL END",
	}

	old := os.Stdout
	r, w, _ := os.Pipe()
	os.Stdout = w

	d := &Driver{}
	for _, in := range inputs {
		_, _ = d.Parse([]byte(in))
	}

	_ = w.Close()
	os.Stdout = old

	var buf bytes.Buffer
	_, _ = io.Copy(&buf, r)
	got := buf.String()

	t.Logf("captured stdout (%d bytes): %q", len(got), got)
	if len(got) != 0 {
		t.Errorf("ANTLR parsing wrote to stdout (corrupts MCP stdio JSON-RPC): %q", got)
	}
}
