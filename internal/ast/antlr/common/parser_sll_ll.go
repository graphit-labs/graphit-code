package antlrcommon

import (
	"os"
	"sync"

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

type silentBailErrorStrategy struct {
	*antlr.BailErrorStrategy
}

func newSilentBailErrorStrategy() *silentBailErrorStrategy {
	return &silentBailErrorStrategy{BailErrorStrategy: antlr.NewBailErrorStrategy()}
}

// ReportError is a no-op (see SilentErrorStrategy.ReportError). Recovery still
// bails via the embedded BailErrorStrategy's RecoverInline/Recover.
func (s *silentBailErrorStrategy) ReportError(_ antlr.Parser, _ antlr.RecognitionException) {
}

const (
	ModeSLL = iota
	ModeLL
)

// staticMu guards the grammars' package-level ATN/DFA/PredictionContextCache
// state. Parsers and lexers READ those fields at construction
// (antlr.NewParserATNSimulator(..., staticData.decisionToDFA, ...)), while a
// cache reset REPLACES them, so the two must not overlap.
//
// A per-pipeline barrier is not sufficient: the daemon runs a pipeline per
// project and the MCP server parses on request, all in one process, so a reset
// driven by one pipeline can race parser construction in another. Parsing takes
// the read lock (concurrent parses stay fully parallel) and resetting takes the
// write lock.
var staticMu sync.RWMutex

// LockParse acquires the shared-state read lock for the duration of one parse,
// including parser/lexer construction. Callers must defer UnlockParse.
func LockParse() { staticMu.RLock() }

// UnlockParse releases the read lock taken by LockParse.
func UnlockParse() { staticMu.RUnlock() }

// WithCacheReset runs fn while holding the exclusive lock, so no parse is
// constructing or running against the static state being replaced.
func WithCacheReset(fn func()) {
	staticMu.Lock()
	defer staticMu.Unlock()
	fn()
}

var resetters []func()

// RegisterCacheResetter records a grammar's ResetCaches function. Called from
// grammar package init(), before any parsing, so no locking is needed.
func RegisterCacheResetter(fn func()) { resetters = append(resetters, fn) }

// ResetAllCaches releases the accumulated DFA / prediction-context caches of
// every linked grammar, holding the exclusive lock so it cannot race a parse.
func ResetAllCaches() {
	WithCacheReset(func() {
		for _, fn := range resetters {
			fn()
		}
	})
}

func FreshDFA(atn *antlr.ATN) []*antlr.DFA {
	dfa := make([]*antlr.DFA, len(atn.DecisionToState))
	for i, state := range atn.DecisionToState {
		dfa[i] = antlr.NewDFA(state, i)
	}
	return dfa
}

var (
	forceSLL    = os.Getenv("GRAPHIT_ANTLR_SLL") == "1"
	forceLLOnly = os.Getenv("GRAPHIT_ANTLR_LL_ONLY") == "1"
)

// Strategy is a grammar's prediction strategy, declared at its call site so each
// grammar states explicitly whether the SLL fast path is safe for it.
type Strategy int

const (
	// SLLThenLL tries fast SLL prediction first, falling back to LL. Use for
	// grammars with no known SLL blowup.
	SLLThenLL Strategy = iota

	// LLOnly skips SLL entirely. Use for grammars whose upstream documentation
	// declares them ambiguous (antlr/grammars-v4 says exactly this for PL/SQL —
	// "The grammar is ambiguous, but generally performs well" — and for
	// PostgreSQL — "The grammar is ambiguous"), because SLL prediction on an
	// ambiguous grammar can grow without bound and kill the process.
	LLOnly
)

func Parse(strategy Strategy, p antlr.Parser, parse func() antlr.ParseTree, configure func(mode int)) antlr.ParseTree {
	useSLL := strategy == SLLThenLL
	if forceSLL {
		useSLL = true
	}
	if forceLLOnly {
		useSLL = false
	}

	if useSLL {
		if tree := trySLLParse(p, parse, configure); tree != nil {
			return tree
		}
	}
	return llParse(parse, configure)
}

// trySLLParse attempts SLL prediction, returning nil when it did not produce a
// clean parse so the caller falls back to full-context LL.
//
// Checking p.HasError() is essential: antlr4-go does NOT panic on a recognition
// error even under BailErrorStrategy — Recover ends with
// recognizer.SetError(...), a flag rather than a panic — so relying on recover()
// alone accepted the SLL tree of an input SLL had actually failed to parse, and
// the LL stage never ran.
func trySLLParse(p antlr.Parser, parse func() antlr.ParseTree, configure func(mode int)) (tree antlr.ParseTree) {
	defer func() {
		if r := recover(); r != nil {
			tree = nil
		}
	}()
	configure(ModeSLL)
	tree = parse()
	if p != nil && p.HasError() {
		return nil
	}
	return tree
}

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
		p.SetErrorHandler(newSilentBailErrorStrategy())
	case ModeLL:
		p.GetInterpreter().SetPredictionMode(antlr.PredictionModeLL)
		p.SetErrorHandler(NewSilentErrorStrategy())
	}
}
