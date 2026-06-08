package shared

import (
	"github.com/antlr4-go/antlr/v4"
)

// SilentErrorStrategy wraps DefaultErrorStrategy but is used together with
// the stdout redirect in Run() to ensure no ANTLR error output leaks to
// the IPC wire. The DefaultErrorStrategy.ReportError() calls fmt.Println
// for unknown error types; the stdout redirect absorbs that. This strategy
// exists as defense-in-depth and for documentation clarity.
type SilentErrorStrategy struct {
	*antlr.DefaultErrorStrategy
}

// NewSilentErrorStrategy creates an error strategy safe for IPC use.
func NewSilentErrorStrategy() *SilentErrorStrategy {
	return &SilentErrorStrategy{
		DefaultErrorStrategy: antlr.NewDefaultErrorStrategy(),
	}
}

// ReportError overrides DefaultErrorStrategy to skip reporting when already
// in recovery mode, then delegates to the default implementation.
// The fmt.Println inside the default is harmlessly absorbed by the
// stdout → /dev/null redirect set up by Run().
func (s *SilentErrorStrategy) ReportError(recognizer antlr.Parser, e antlr.RecognitionException) {
	if s.InErrorRecoveryMode(recognizer) {
		return
	}
	s.DefaultErrorStrategy.ReportError(recognizer, e)
}

// ParseSLLThenLL implements two-stage SLL→LL parsing with panic recovery.
//
//   - Stage 1: SLL prediction with BailErrorStrategy (fast path, O(n)).
//   - Stage 2: LL prediction with SilentErrorStrategy (full recovery).
//
// Parameters:
//   - lexer: the grammar's lexer (error listeners will be removed)
//   - setup: called to configure a parser before each parse attempt.
//     Receives tokens and returns (parser interface, buildParseTrees pointer, start rule invoker).
//
// This is the recommended parsing strategy for all grammars.
func ParseSLLThenLL(lexer antlr.Lexer, parse func() antlr.ParseTree, configure func(mode int)) antlr.ParseTree {
	// Stage 1: SLL (fast path)
	tree := trySLLParse(parse, configure)
	if tree != nil {
		return tree
	}

	// Stage 2: LL (full recovery)
	return llParse(parse, configure)
}

const (
	ModeSLL = iota
	ModeLL
)

// trySLLParse attempts SLL prediction. Returns nil on failure
// (BailErrorStrategy panics on ambiguity).
func trySLLParse(parse func() antlr.ParseTree, configure func(mode int)) (tree antlr.ParseTree) {
	defer func() {
		if r := recover(); r != nil {
			tree = nil
		}
	}()

	configure(ModeSLL)
	tree = parse()
	return tree
}

// llParse runs LL prediction with SilentErrorStrategy (full error recovery
// without stdout pollution).
func llParse(parse func() antlr.ParseTree, configure func(mode int)) (tree antlr.ParseTree) {
	defer func() {
		if r := recover(); r != nil {
			tree = nil
		}
	}()

	configure(ModeLL)
	return parse()
}

// ConfigureParser sets up a parser for the given mode.
// This is a helper for grammar main.go files to use in their configure callback.
func ConfigureParser(p antlr.Parser, tokens *antlr.CommonTokenStream, buildParseTrees *bool, mode int) {
	tokens.Seek(0)
	p.RemoveErrorListeners()
	*buildParseTrees = true

	switch mode {
	case ModeSLL:
		p.GetInterpreter().SetPredictionMode(antlr.PredictionModeSLL)
		p.SetErrorHandler(antlr.NewBailErrorStrategy())
	case ModeLL:
		p.GetInterpreter().SetPredictionMode(antlr.PredictionModeLL)
		p.SetErrorHandler(NewSilentErrorStrategy())
	}
}
