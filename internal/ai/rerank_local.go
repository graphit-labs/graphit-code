package ai

import (
	"context"
	"fmt"
	"log/slog"
	"math"
	"runtime"
	"sync"

	ort "github.com/yalue/onnxruntime_go"

	tokenizer "github.com/sugarme/tokenizer"
	"github.com/sugarme/tokenizer/pretrained"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
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

	inputs         []onnxInputBinding
	output         string
	modelName      string
	maxTokens      int
	maxBatch       int
	scoreTransform string
	positiveClass  *int

	closed bool
}

func (r *CrossEncoderReranker) log() *slog.Logger { return slogutil.Resolve(r.Logger) }

// NewCrossEncoderReranker resolves the selected rerank manifest. Missing artifacts are downloaded
// only when its fetch policy permits it; callers reach this path only when reranking is enabled.
func NewCrossEncoderReranker(ctx context.Context) (*CrossEncoderReranker, error) {
	execution, err := configuredONNXExecution(ModelTaskRerank)
	if err != nil {
		return nil, err
	}
	if err := initONNXRuntime(); err != nil {
		return nil, fmt.Errorf("init ONNX Runtime: %w", err)
	}
	model, err := LoadConfiguredModel(ModelTaskRerank)
	if err != nil {
		return nil, fmt.Errorf("resolve local rerank model: %w", err)
	}
	paths, err := model.Ensure(ctx)
	if err != nil {
		return nil, err
	}
	return newCrossEncoderFrom(model, paths[model.Manifest.Runtime.Entrypoints["model"]], paths[model.Manifest.Tokenizer.Artifact], execution)
}

// NewCrossEncoderRerankerIfPresent loads the model ONLY if it is already on disk.
//
// This is for a caller that wants reranking when it is free and does not want to trigger a
// download as a side effect of a query — a daemon starting up, for instance. It returns
// (nil, nil) when the model is absent, which the caller reads as "no reranking", not as an error.
func NewCrossEncoderRerankerIfPresent() (*CrossEncoderReranker, error) {
	execution, err := configuredONNXExecution(ModelTaskRerank)
	if err != nil {
		return nil, err
	}
	model, err := LoadConfiguredModel(ModelTaskRerank)
	if err != nil {
		return nil, err
	}
	if !model.Present() {
		return nil, nil
	}
	modelPath, err := model.EntrypointPath("model")
	if err != nil {
		return nil, err
	}
	tokenizerPath, err := model.ArtifactPath(model.Manifest.Tokenizer.Artifact)
	if err != nil {
		return nil, err
	}
	return newCrossEncoderFrom(model, modelPath, tokenizerPath, execution)
}

func newCrossEncoderFrom(model *ResolvedModel, modelPath, tokenizerPath string, execution ONNXExecutionConfig) (*CrossEncoderReranker, error) {
	if err := initONNXRuntime(); err != nil {
		return nil, err
	}

	tk, err := pretrained.FromFile(tokenizerPath)
	if err != nil {
		return nil, fmt.Errorf("load reranker tokenizer from %s: %w", tokenizerPath, err)
	}

	contract, err := inspectONNXTextContract(model.Manifest, modelPath)
	if err != nil {
		return nil, err
	}
	inputNames := make([]string, len(contract.Inputs))
	for i, input := range contract.Inputs {
		inputNames[i] = input.Name
	}

	session, _, err := newDynamicONNXSession(modelPath, inputNames, []string{contract.Output}, execution, func(opts *ort.SessionOptions) error {
		if n := runtime.NumCPU() - 2; n > 0 {
			if err := opts.SetIntraOpNumThreads(n); err != nil {
				return fmt.Errorf("set reranker intra-op threads: %w", err)
			}
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("open reranker model %s: %w", modelPath, err)
	}

	return &CrossEncoderReranker{
		tk: tk, session: session,
		inputs:         contract.Inputs,
		output:         contract.Output,
		modelName:      model.Manifest.Name,
		maxTokens:      model.Manifest.Text.MaxTokens,
		maxBatch:       model.Manifest.Inference.MaxBatch,
		scoreTransform: model.Manifest.Inference.ScoreTransform,
		positiveClass:  model.Manifest.Inference.PositiveClass,
	}, nil
}

// Name identifies the reranker in logs and in a Hit's Mode.
func (r *CrossEncoderReranker) Name() string { return r.modelName }

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
	for start := 0; start < len(candidates); start += r.maxBatch {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		end := start + r.maxBatch
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
		if len(tokenIDs) > r.maxTokens {
			tokenIDs = tokenIDs[:r.maxTokens]
			if len(attention) > r.maxTokens {
				attention = attention[:r.maxTokens]
			}
			if len(typeIDs) > r.maxTokens {
				typeIDs = typeIDs[:r.maxTokens]
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

	for _, binding := range r.inputs {
		var data []int64
		switch binding.Role {
		case "input_ids":
			data = flatIDs
		case "attention_mask":
			data = flatMask
		case "token_type_ids":
			data = flatTypes
		default:
			return nil, fmt.Errorf("reranker model wants an input role this code does not build: %q", binding.Role)
		}
		tensor, tErr := ort.NewTensor(shape, data)
		if tErr != nil {
			return nil, fmt.Errorf("reranker %s tensor: %w", binding.Name, tErr)
		}
		byName[binding.Name] = tensor
	}

	in := make([]ort.Value, 0, len(r.inputs))
	for _, binding := range r.inputs {
		in = append(in, byName[binding.Name])
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
	if stride == 0 || stride*len(batch) != len(data) {
		return nil, fmt.Errorf("reranker output %q has %d values that do not align with %d pairs", r.output, len(data), len(batch))
	}
	scores := make([]float64, len(batch))
	for i := range batch {
		if failed[i] {
			scores[i] = math.Inf(-1)
			continue
		}
		row := data[i*stride : (i+1)*stride]
		v, transformErr := r.transformScore(row)
		if transformErr != nil {
			return nil, transformErr
		}
		scores[i] = v
	}
	return scores, nil
}

func (r *CrossEncoderReranker) transformScore(logits []float32) (float64, error) {
	if len(logits) == 0 {
		return 0, fmt.Errorf("reranker output %q returned an empty score row", r.output)
	}
	class := 0
	if r.positiveClass != nil {
		class = *r.positiveClass
	} else if r.scoreTransform == "auto" && len(logits) == 2 {
		class = 1
	}
	if class < 0 || class >= len(logits) {
		return 0, fmt.Errorf("reranker positive_class %d is outside output width %d", class, len(logits))
	}
	switch r.scoreTransform {
	case "auto", "none":
		return float64(logits[class]), nil
	case "sigmoid":
		value := float64(logits[class])
		return 1 / (1 + math.Exp(-value)), nil
	case "softmax":
		maxValue := float64(logits[0])
		for _, logit := range logits[1:] {
			if value := float64(logit); value > maxValue {
				maxValue = value
			}
		}
		var denominator float64
		for _, logit := range logits {
			denominator += math.Exp(float64(logit) - maxValue)
		}
		return math.Exp(float64(logits[class])-maxValue) / denominator, nil
	default:
		return 0, fmt.Errorf("unsupported reranker score transform %q", r.scoreTransform)
	}
}

func (r *CrossEncoderReranker) encodePair(query, candidate string) (enc *tokenizer.Encoding, err error) {
	defer func() {
		if rec := recover(); rec != nil {
			enc, err = nil, fmt.Errorf("tokenizer panicked: %v", rec)
		}
	}()
	return r.tk.EncodePair(query, candidate, true)
}
