package postgresql

import (
	"github.com/antlr4-go/antlr/v4"
)

// SilentErrorStrategy wraps DefaultErrorStrategy to suppress output during recovery.
type SilentErrorStrategy struct {
	*antlr.DefaultErrorStrategy
}

// NewSilentErrorStrategy creates an error strategy.
func NewSilentErrorStrategy() *SilentErrorStrategy {
	return &SilentErrorStrategy{
		DefaultErrorStrategy: antlr.NewDefaultErrorStrategy(),
	}
}

// ReportError overrides DefaultErrorStrategy to skip reporting when already
// in recovery mode, then delegates to the default implementation.
func (s *SilentErrorStrategy) ReportError(recognizer antlr.Parser, e antlr.RecognitionException) {
	if s.InErrorRecoveryMode(recognizer) {
		return
	}
	s.DefaultErrorStrategy.ReportError(recognizer, e)
}

// ParseSLLThenLL implements two-stage SLL→LL parsing with panic recovery.
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

// llParse runs LL prediction with SilentErrorStrategy.
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
