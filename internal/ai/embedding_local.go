package ai

import (
	"context"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"sync"

	"github.com/sugarme/tokenizer/pretrained"
	ort "github.com/yalue/onnxruntime_go"

	tokenizer "github.com/sugarme/tokenizer"
)

const (
	localModelName = "CodeRankEmbed-137M-INT8"

	maxSeqLen = 512

	queryPrefix = "Represent this query for searching relevant code: "
)

type localEmbeddingClient struct {
	tk      *tokenizer.Tokenizer
	session *ort.DynamicAdvancedSession

	mu sync.Mutex
}

var ortInitOnce sync.Once
var ortInitErr error

func initONNXRuntime() error {
	ortInitOnce.Do(func() {
		libPath := findORTLibrary()
		if libPath != "" {
			ort.SetSharedLibraryPath(libPath)
		}
		ortInitErr = ort.InitializeEnvironment()
	})
	return ortInitErr
}

func findORTLibrary() string {
	var libName string
	switch runtime.GOOS {
	case "windows":
		libName = "onnxruntime.dll"
	case "darwin":
		libName = "libonnxruntime.dylib"
	default:
		libName = "libonnxruntime.so"
	}

	if exe, err := os.Executable(); err == nil {
		candidate := filepath.Join(filepath.Dir(exe), libName)
		if _, err := os.Stat(candidate); err == nil {
			return candidate
		}
	}

	var envVar string
	switch runtime.GOOS {
	case "darwin":
		envVar = "DYLD_LIBRARY_PATH"
	case "windows":
		envVar = "PATH"
	default:
		envVar = "LD_LIBRARY_PATH"
	}
	if paths := os.Getenv(envVar); paths != "" {
		for _, dir := range filepath.SplitList(paths) {
			candidate := filepath.Join(dir, libName)
			if _, err := os.Stat(candidate); err == nil {
				return candidate
			}
		}
	}

	return ""
}

func NewLocalEmbeddingClient() (*localEmbeddingClient, error) {

	mgr, err := NewModelManager()
	if err != nil {
		return nil, fmt.Errorf("model manager: %w", err)
	}

	modelPath, tokenizerPath, err := mgr.EnsureModel(context.Background())
	if err != nil {
		return nil, fmt.Errorf("ensure model: %w", err)
	}

	if err := initONNXRuntime(); err != nil {
		return nil, fmt.Errorf("init ONNX Runtime: %w", err)
	}

	tk, err := pretrained.FromFile(tokenizerPath)
	if err != nil {
		return nil, fmt.Errorf("load tokenizer from %s: %w", tokenizerPath, err)
	}

	inputNames := []string{"input_ids", "attention_mask"}
	outputNames := []string{"sentence_embedding"}

	// Bound ONNX Runtime's thread usage. With nil options it defaults its
	// intra-op pool to NumCPU native threads, so the background embedding loop
	// saturates every core while the machine is otherwise idle. Cap intra-op to
	// half the cores (min 1) and inter-op to 1 to stay machine-friendly.
	// GRAPHIT_EMBED_THREADS overrides.
	var sessOpts *ort.SessionOptions
	if opts, optErr := ort.NewSessionOptions(); optErr == nil {
		_ = opts.SetIntraOpNumThreads(boundedEmbedThreads())
		_ = opts.SetInterOpNumThreads(1)
		sessOpts = opts
		defer func() { _ = opts.Destroy() }()
	}

	session, err := ort.NewDynamicAdvancedSession(
		modelPath,
		inputNames,
		outputNames,
		sessOpts,
	)
	if err != nil {
		return nil, fmt.Errorf("create ONNX session: %w", err)
	}

	return &localEmbeddingClient{
		tk:      tk,
		session: session,
	}, nil
}

// boundedEmbedThreads returns the ONNX Runtime intra-op thread count: half the
// cores (min 1, max 8) so background embedding does not monopolize the machine.
// GRAPHIT_EMBED_THREADS overrides.
func boundedEmbedThreads() int {
	if s := os.Getenv("GRAPHIT_EMBED_THREADS"); s != "" {
		if n, err := strconv.Atoi(s); err == nil && n > 0 {
			return n
		}
	}
	n := runtime.NumCPU() / 2
	if n < 1 {
		n = 1
	}
	if n > 8 {
		n = 8
	}
	return n
}

func (c *localEmbeddingClient) ModelName() string { return localModelName }

func (c *localEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	vecs, err := c.EmbedBatch(ctx, []string{text})
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("empty embedding response")
	}
	return vecs[0], nil
}

func (c *localEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	batchSize := len(texts)

	allIDs := make([][]int, batchSize)
	allMasks := make([][]int, batchSize)
	maxLen := 0

	for i, text := range texts {
		enc, err := c.tk.EncodeSingle(text, true)
		if err != nil {
			return nil, fmt.Errorf("tokenize text %d: %w", i, err)
		}

		ids := enc.GetIds()
		mask := enc.GetAttentionMask()

		if len(ids) > maxSeqLen {
			ids = ids[:maxSeqLen]
			mask = mask[:maxSeqLen]
		}

		allIDs[i] = ids
		allMasks[i] = mask

		if len(ids) > maxLen {
			maxLen = len(ids)
		}
	}

	inputIDs := make([]int64, batchSize*maxLen)
	attentionMask := make([]int64, batchSize*maxLen)

	for i := 0; i < batchSize; i++ {
		for j := 0; j < len(allIDs[i]); j++ {
			idx := i*maxLen + j
			inputIDs[idx] = int64(allIDs[i][j])
			attentionMask[idx] = int64(allMasks[i][j])
		}

	}

	shape := ort.Shape{int64(batchSize), int64(maxLen)}

	inputIDsTensor, err := ort.NewTensor(shape, inputIDs)
	if err != nil {
		return nil, fmt.Errorf("create input_ids tensor: %w", err)
	}
	defer func() { _ = inputIDsTensor.Destroy() }()

	attMaskTensor, err := ort.NewTensor(shape, attentionMask)
	if err != nil {
		return nil, fmt.Errorf("create attention_mask tensor: %w", err)
	}
	defer func() { _ = attMaskTensor.Destroy() }()

	outputs := []ort.Value{nil}
	c.mu.Lock()
	err = c.session.Run(
		[]ort.Value{inputIDsTensor, attMaskTensor},
		outputs,
	)
	c.mu.Unlock()
	if err != nil {
		return nil, fmt.Errorf("ONNX inference: %w", err)
	}
	if outputs[0] == nil {
		return nil, fmt.Errorf("no output from ONNX model")
	}
	defer func() { _ = outputs[0].Destroy() }()

	outputTensor, ok := outputs[0].(*ort.Tensor[float32])
	if !ok {
		return nil, fmt.Errorf("unexpected output tensor type")
	}
	outputData := outputTensor.GetData()
	hiddenDim := EmbeddingDimensions

	results := make([][]float32, batchSize)
	for i := 0; i < batchSize; i++ {
		vec := make([]float32, hiddenDim)
		copy(vec, outputData[i*hiddenDim:(i+1)*hiddenDim])

		var norm float64
		for k := 0; k < hiddenDim; k++ {
			norm += float64(vec[k]) * float64(vec[k])
		}
		norm = math.Sqrt(norm)
		if norm > 0 {
			for k := 0; k < hiddenDim; k++ {
				vec[k] = float32(float64(vec[k]) / norm)
			}
		}

		results[i] = vec
	}

	return results, nil
}

func (c *localEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	return c.Embed(ctx, queryPrefix+query)
}

func (c *localEmbeddingClient) Close() {
	if c.session != nil {
		_ = c.session.Destroy()
	}
}
