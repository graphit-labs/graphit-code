package antlrcommon

import (
	"github.com/antlr4-go/antlr/v4"
)

// SilentErrorStrategy suppresses ANTLR error reporting entirely.
//
// antlr's DefaultErrorStrategy.ReportError has a default switch case that
// unconditionally does fmt.Println("unknown recognition error type: ...") to
// STDOUT for any RecognitionException whose concrete type is not
// *NoViableAltException / *InputMisMatchException / *FailedPredicateException.
// This process runs as a stdio MCP server, where ANY write to stdout corrupts
// the JSON-RPC framing. ReportError is therefore a full no-op: error RECOVERY is
// performed by the inherited Recover / RecoverInline / Sync methods (none of
// which write to stdout), so suppressing reporting is safe.
type SilentErrorStrategy struct {
	*antlr.DefaultErrorStrategy
}

// NewSilentErrorStrategy creates a SilentErrorStrategy.
func NewSilentErrorStrategy() *SilentErrorStrategy {
	return &SilentErrorStrategy{
		DefaultErrorStrategy: antlr.NewDefaultErrorStrategy(),
	}
}

// ReportError is a no-op. It deliberately never delegates to
// DefaultErrorStrategy.ReportError, whose default case writes to stdout.
func (s *SilentErrorStrategy) ReportError(_ antlr.Parser, _ antlr.RecognitionException) {
}

const (
	ModeSLL = iota
	ModeLL
)

// ParseSLLThenLL implements two-stage SLL→LL parsing with panic recovery.
// Stage 1 tries fast SLL prediction (BailErrorStrategy); on failure stage 2
// runs full-context LL prediction with the silent error strategy.
func ParseSLLThenLL(parse func() antlr.ParseTree, configure func(mode int)) antlr.ParseTree {
	if tree := trySLLParse(parse, configure); tree != nil {
		return tree
	}
	return llParse(parse, configure)
}

// trySLLParse attempts SLL prediction. Returns nil on failure.
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

// llParse runs full-context LL prediction with the SilentErrorStrategy.
func llParse(parse func() antlr.ParseTree, configure func(mode int)) (tree antlr.ParseTree) {
	defer func() {
		if r := recover(); r != nil {
			tree = nil
		}
	}()
	configure(ModeLL)
	return parse()
}

// ConfigureParser sets up a parser for the given prediction mode.
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
