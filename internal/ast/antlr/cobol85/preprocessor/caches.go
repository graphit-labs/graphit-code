package preprocessor

import (
	"github.com/antlr4-go/antlr/v4"
	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// ResetCaches discards this grammar's accumulated ANTLR DFA and
// prediction-context caches (parser and lexer), which otherwise grow unbounded
// across parses and are never evicted. The deserialized ATN is kept — it is
// read-only and expensive to rebuild.
//
// NOT safe to call while a parse is in flight: it replaces shared static state.
// Callers must invoke it at a barrier where no goroutine is parsing.
func ResetCaches() {
	if p := &Cobol85PreprocessorParserStaticData; p.atn != nil {
		p.decisionToDFA = antlrcommon.FreshDFA(p.atn)
		p.PredictionContextCache = antlr.NewPredictionContextCache()
	}
	if l := &Cobol85PreprocessorLexerLexerStaticData; l.atn != nil {
		l.decisionToDFA = antlrcommon.FreshDFA(l.atn)
		l.PredictionContextCache = antlr.NewPredictionContextCache()
	}
}

func init() { antlrcommon.RegisterCacheResetter(ResetCaches) }
