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

const (
	rerankMaxLen = 512

	rerankMaxBatch = 16
)

type pairEncoder interface {
	EncodePair(string, string, ...bool) (*tokenizer.Encoding, error)
}

// CrossEncoderReranker scores (query, candidate) pairs with a local ONNX cross-encoder.
type CrossEncoderReranker struct {
	Logger *slog.Logger

	tk      pairEncoder
	session *ort.DynamicAdvancedSession
	mu      sync.Mutex

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
	if n := runtime.NumCPU() - 2; n > 0 {
		_ = opts.SetIntraOpNumThreads(n)
	}

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

	stride := len(data) / len(batch)
	scores := make([]float64, len(batch))
	for i := range batch {
		if failed[i] {
			scores[i] = math.Inf(-1)
			continue
		}
		v := float64(data[i*stride])
		if stride == 2 {
			v = float64(data[i*stride+1])
		}
		scores[i] = v
	}
	return scores, nil
}

func (r *CrossEncoderReranker) encodePair(query, candidate string) (enc *tokenizer.Encoding, err error) {
	defer func() {
		if rec := recover(); rec != nil {
			enc, err = nil, fmt.Errorf("tokenizer panicked: %v", rec)
		}
	}()
	return r.tk.EncodePair(query, candidate, true)
}
