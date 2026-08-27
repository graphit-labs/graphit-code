package ai

import (
	"context"
	"fmt"
	"log/slog"
	"math"
	"path/filepath"
	"runtime"
	"sync"

	ort "github.com/yalue/onnxruntime_go"

	tokenizer "github.com/sugarme/tokenizer"
	"github.com/sugarme/tokenizer/pretrained"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// The cross-encoder reranker.
//
// HOW IT DIFFERS FROM THE EMBEDDER, which is the only interesting part: a bi-encoder reads one
// text and returns a vector, and similarity is computed afterwards from two vectors. A
// cross-encoder reads the QUERY AND THE CANDIDATE TOGETHER, as one sequence, and returns a single
// relevance score. That is why it is better and why it is expensive — it cannot be precomputed or
// cached by content hash, because the score belongs to the pair, not to the document.
//
// So there is no pooling and no L2 normalisation here. The model emits one logit per pair; the
// score is that number, and ordering by it is the whole output.
//
// SEQUENCE LAYOUT. The pair is encoded as a single sequence with the tokenizer's pair encoding,
// which inserts the separator the model was trained with. Getting this wrong does not error — it
// produces scores that look plausible and rank badly — so it goes through EncodePair rather than
// through string concatenation.

const (
	// rerankMaxLen bounds the pair sequence. The model accepts 1024; 512 is used because a
	// candidate here is an identifier plus a docstring plus a gram bag, which does not approach
	// the limit, and the cost of the attention matrix is quadratic in this number.
	rerankMaxLen = 512

	// rerankMaxBatch bounds one inference call. Reranking runs on the query path, so the whole
	// candidate set is not sent at once: a bounded batch keeps peak memory flat and lets a
	// cancelled context take effect between batches rather than only at the end.
	rerankMaxBatch = 16
)

// pairEncoder is the part of *tokenizer.Tokenizer this client uses.
//
// Narrowed to an interface for the same reason the embedder narrows its own: it makes the panic
// path testable. The tokenizer panics rather than returning an error on some inputs, and the only
// way to prove the recovery works is to inject something that panics on demand.
type pairEncoder interface {
	EncodePair(string, string, ...bool) (*tokenizer.Encoding, error)
}

// CrossEncoderReranker scores (query, candidate) pairs with a local ONNX cross-encoder.
type CrossEncoderReranker struct {
	Logger *slog.Logger

	tk      pairEncoder
	session *ort.DynamicAdvancedSession
	mu      sync.Mutex

	// inputNames is what the model declared, in its own order, and wantsTypeIDs says whether
	// token_type_ids is among them. Both come from the model file rather than from an assumption
	// about its architecture — see newCrossEncoderFrom.
	inputNames   []string
	wantsTypeIDs bool

	closed bool
}

func (r *CrossEncoderReranker) log() *slog.Logger { return slogutil.Resolve(r.Logger) }

// NewCrossEncoderReranker loads the model, DOWNLOADING IT IF IT IS ABSENT.
//
// Calling this is the commitment to the 1.04 GiB bundle, so nothing calls it unless reranking was
// explicitly enabled. It is not called by setup, not called on the indexing path, and not called
// to answer a query with reranking off.
func NewCrossEncoderReranker(ctx context.Context) (*CrossEncoderReranker, error) {
	mgr, err := NewRerankModelManager()
	if err != nil {
		return nil, err
	}
	modelPath, tokenizerPath, err := mgr.Ensure(ctx)
	if err != nil {
		return nil, err
	}
	return newCrossEncoderFrom(modelPath, tokenizerPath)
}

// NewCrossEncoderRerankerIfPresent loads the model ONLY if it is already on disk.
//
// This is for a caller that wants reranking when it is free and does not want to trigger a
// download as a side effect of a query — a daemon starting up, for instance. It returns
// (nil, nil) when the model is absent, which the caller reads as "no reranking", not as an error.
func NewCrossEncoderRerankerIfPresent() (*CrossEncoderReranker, error) {
	mgr, err := NewRerankModelManager()
	if err != nil {
		return nil, err
	}
	if !mgr.Present() {
		return nil, nil
	}
	return newCrossEncoderFrom(
		filepath.Join(mgr.CacheDir(), modelFileName),
		filepath.Join(mgr.CacheDir(), tokenizerFileName))
}

func newCrossEncoderFrom(modelPath, tokenizerPath string) (*CrossEncoderReranker, error) {
	// The runtime is process-global and the embedder already owns its initialisation, guarded by
	// a sync.Once. Reusing it rather than adding a second initialiser is what keeps the library
	// path resolved in exactly one place.
	if err := initONNXRuntime(); err != nil {
		return nil, err
	}

	tk, err := pretrained.FromFile(tokenizerPath)
	if err != nil {
		return nil, fmt.Errorf("load reranker tokenizer from %s: %w", tokenizerPath, err)
	}

	opts, err := ort.NewSessionOptions()
	if err != nil {
		return nil, fmt.Errorf("reranker session options: %w", err)
	}
	defer func() { _ = opts.Destroy() }()
	// One thread per core, minus the two the rest of the process needs to stay responsive while
	// an inference batch is running. The embedder makes the same trade.
	if n := runtime.NumCPU() - 2; n > 0 {
		_ = opts.SetIntraOpNumThreads(n)
	}

	// THE INPUT SET IS DISCOVERED, NOT ASSUMED, and this is not defensive coding — it is the
	// difference between two architectures that are both valid rerankers.
	//
	// MEASURED: a BERT cross-encoder (ms-marco-MiniLM) REQUIRES `token_type_ids`, because that is
	// how it tells the query segment from the document segment, and running it without one fails
	// with "Missing Input: token_type_ids" from inside a Gather node. An XLM-RoBERTa one
	// (bge-reranker-base) has no such input and passing it would fail the other way. A bi-encoder
	// needs neither, which is why the embedder beside this file gets away with a fixed pair.
	//
	// So the model is asked what it wants.
	inputs, outputs, err := ort.GetInputOutputInfo(modelPath)
	if err != nil {
		return nil, fmt.Errorf("read the reranker model's signature from %s: %w", modelPath, err)
	}
	inputNames := make([]string, 0, len(inputs))
	wantsTypeIDs := false
	for _, in := range inputs {
		inputNames = append(inputNames, in.Name)
		if in.Name == "token_type_ids" {
			wantsTypeIDs = true
		}
	}
	if len(outputs) == 0 {
		return nil, fmt.Errorf("reranker model %s declares no output", modelPath)
	}
	outputNames := []string{outputs[0].Name}

	session, err := ort.NewDynamicAdvancedSession(modelPath, inputNames, outputNames, opts)
	if err != nil {
		return nil, fmt.Errorf("open reranker model %s: %w", modelPath, err)
	}

	return &CrossEncoderReranker{
		tk: tk, session: session,
		inputNames:   inputNames,
		wantsTypeIDs: wantsTypeIDs,
	}, nil
}

// Name identifies the reranker in logs and in a Hit's Mode.
func (r *CrossEncoderReranker) Name() string { return rerankModelName }

// Close releases the session.
func (r *CrossEncoderReranker) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed || r.session == nil {
		return nil
	}
	r.closed = true
	return r.session.Destroy()
}

// Score returns one relevance score per candidate, in the order given.
//
// A candidate that cannot be tokenised scores the lowest possible value rather than failing the
// batch: one malformed document must not cost the user the whole result set.
func (r *CrossEncoderReranker) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}
	if r.session == nil || r.closed {
		return nil, fmt.Errorf("reranker is closed")
	}

	out := make([]float64, len(candidates))
	for start := 0; start < len(candidates); start += rerankMaxBatch {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		end := start + rerankMaxBatch
		if end > len(candidates) {
			end = len(candidates)
		}
		scores, err := r.scoreBatch(query, candidates[start:end])
		if err != nil {
			return nil, err
		}
		copy(out[start:end], scores)
	}
	return out, nil
}

func (r *CrossEncoderReranker) scoreBatch(query string, batch []string) ([]float64, error) {
	ids := make([][]int, len(batch))
	masks := make([][]int, len(batch))
	types := make([][]int, len(batch))
	failed := make([]bool, len(batch))

	for i, cand := range batch {
		enc, err := r.encodePair(query, cand)
		if err != nil {
			r.log().Debug("reranker could not tokenise a candidate", "error", err)
			failed[i] = true
			// A single padding token keeps the tensor rectangular; the score is discarded below.
			ids[i], masks[i], types[i] = []int{0}, []int{0}, []int{0}
			continue
		}
		tokenIDs, attention, typeIDs := enc.Ids, enc.AttentionMask, enc.TypeIds
		if len(tokenIDs) > rerankMaxLen {
			tokenIDs = tokenIDs[:rerankMaxLen]
			attention = attention[:rerankMaxLen]
			if len(typeIDs) > rerankMaxLen {
				typeIDs = typeIDs[:rerankMaxLen]
			}
		}
		ids[i], masks[i], types[i] = tokenIDs, attention, typeIDs
	}

	maxLen := 1
	for _, s := range ids {
		if len(s) > maxLen {
			maxLen = len(s)
		}
	}

	flatIDs := make([]int64, len(batch)*maxLen)
	flatMask := make([]int64, len(batch)*maxLen)
	flatTypes := make([]int64, len(batch)*maxLen)
	for i := range batch {
		for j := 0; j < len(ids[i]) && j < len(masks[i]); j++ {
			flatIDs[i*maxLen+j] = int64(ids[i][j])
			flatMask[i*maxLen+j] = int64(masks[i][j])
			if j < len(types[i]) {
				flatTypes[i*maxLen+j] = int64(types[i][j])
			}
		}
	}

	shape := ort.Shape{int64(len(batch)), int64(maxLen)}
	byName := map[string]ort.Value{}
	defer func() {
		for _, v := range byName {
			_ = v.Destroy()
		}
	}()

	for _, name := range r.inputNames {
		var data []int64
		switch name {
		case "input_ids":
			data = flatIDs
		case "attention_mask":
			data = flatMask
		case "token_type_ids":
			data = flatTypes
		default:
			return nil, fmt.Errorf("reranker model wants an input this code does not build: %q", name)
		}
		tensor, tErr := ort.NewTensor(shape, data)
		if tErr != nil {
			return nil, fmt.Errorf("reranker %s tensor: %w", name, tErr)
		}
		byName[name] = tensor
	}

	// In the order the model declared them.
	in := make([]ort.Value, 0, len(r.inputNames))
	for _, name := range r.inputNames {
		in = append(in, byName[name])
	}

	outputs := []ort.Value{nil}
	r.mu.Lock()
	err := r.session.Run(in, outputs)
	r.mu.Unlock()
	if err != nil {
		return nil, fmt.Errorf("reranker inference: %w", err)
	}
	if outputs[0] == nil {
		return nil, fmt.Errorf("reranker produced no output")
	}
	defer func() { _ = outputs[0].Destroy() }()

	tensor, ok := outputs[0].(*ort.Tensor[float32])
	if !ok {
		return nil, fmt.Errorf("reranker output is %T, want float32", outputs[0])
	}
	data := tensor.GetData()
	if len(data) < len(batch) {
		return nil, fmt.Errorf("reranker returned %d logits for %d pairs", len(data), len(batch))
	}

	// One logit per pair. A cross-encoder's head emits a single value, so the stride is
	// len(data)/len(batch) — one for this model, and reading it rather than assuming it means a
	// model with a two-class head still scores correctly.
	stride := len(data) / len(batch)
	scores := make([]float64, len(batch))
	for i := range batch {
		if failed[i] {
			scores[i] = math.Inf(-1)
			continue
		}
		v := float64(data[i*stride])
		if stride == 2 {
			// A two-class head emits [irrelevant, relevant]; the second is the score.
			v = float64(data[i*stride+1])
		}
		scores[i] = v
	}
	return scores, nil
}

// encodePair runs the tokenizer's pair encoding, recovering from a panic inside it.
//
// SAFETY: the tokenizer panics rather than returning an error on some inputs — the embedder hit
// this and carries the same guard. A panic here would take down whatever is serving the query.
func (r *CrossEncoderReranker) encodePair(query, candidate string) (enc *tokenizer.Encoding, err error) {
	defer func() {
		if rec := recover(); rec != nil {
			enc, err = nil, fmt.Errorf("tokenizer panicked: %v", rec)
		}
	}()
	return r.tk.EncodePair(query, candidate, true)
}
