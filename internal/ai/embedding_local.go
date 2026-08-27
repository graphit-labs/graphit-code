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

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/sugarme/tokenizer/pretrained"
	ort "github.com/yalue/onnxruntime_go"

	tokenizer "github.com/sugarme/tokenizer"
)

const (
	localModelName = "CodeRankEmbed-137M-INT8"

	maxSeqLen = 512

	queryPrefix = "Represent this query for searching relevant code: "
)

// textEncoder is the part of *tokenizer.Tokenizer this client uses. Narrowed to
// an interface so the panic path below can be exercised by a test: the panic
// comes from inside the tokenizer on inputs that cannot be characterized from
// outside it, so what gets tested is this package's containment of it, not the
// upstream bug.
type textEncoder interface {
	EncodeSingle(string, ...bool) (*tokenizer.Encoding, error)
}

type localEmbeddingClient struct {
	tk      textEncoder
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

	// The launcher's EXTRACTED payload, which is the only copy a binary that does not travel
	// with the library can reach.
	//
	// The library ships beside the binary inside the launcher payload, so the check above finds
	// it for a distributed install. It finds nothing for anything else built from this tree: a
	// `go test` binary lives in a temp directory the toolchain made, and `make build-local`
	// produces a bare core. Those got no library, so ort.SetSharedLibraryPath was never called,
	// the binding fell back to its own default name — "onnxruntime.so", not the "lib" form this
	// project ships — and every caller reported the library as missing.
	//
	// It is the same resolution the AST module already does for its query YAMLs and the
	// ladybugstore for its extensions: read what the last install extracted. See
	// runtimeQueriesDir in internal/ast/query_loader.go and ExtensionDir in
	// internal/ladybugstore/extension.go.
	if d := brand.RuntimeDir(version.Version); d != "" {
		candidate := filepath.Join(d, libName)
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

	// ORT FIRST, because it is the cheap check and the model is a ~132 MB download.
	//
	// These two were the other way round, so a machine without the runtime paid for the model
	// before discovering it could not use it — and paid again on the next call, since nothing
	// caches a failure. It was invisible because the callers turn this error into t.Skip or into
	// a silent degradation to keyword-only search: the download had no observable effect except
	// the disk it consumed.
	if err := initONNXRuntime(); err != nil {
		return nil, fmt.Errorf("init ONNX Runtime: %w", err)
	}

	mgr, err := NewModelManager()
	if err != nil {
		return nil, fmt.Errorf("model manager: %w", err)
	}

	modelPath, tokenizerPath, err := mgr.EnsureModel(context.Background())
	if err != nil {
		return nil, fmt.Errorf("ensure model: %w", err)
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

// boundedEmbedThreads returns the ONNX Runtime intra-op thread count, derived
// from the shared CPU budget so background embedding does not monopolize the
// machine or oversubscribe cores alongside the parse and DB thread pools.
// GRAPHIT_EMBED_THREADS overrides.
func boundedEmbedThreads() int {
	if s := os.Getenv("GRAPHIT_EMBED_THREADS"); s != "" {
		if n, err := strconv.Atoi(s); err == nil && n > 0 {
			return n
		}
	}
	return sysutil.CPUBudget()
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

// encodeSingle tokenizes one text, turning a panic from the tokenizer into an
// error.
//
// SAFETY: sugarme/tokenizer v0.3.0 panics on some inputs instead of failing.
// NormalizedString.Slice derives the original-string range with ConvertOffset,
// which — unlike IntoFullRange, used on the other branch — does not clamp to the
// string's length, so RangeOriginal can slice one byte past the end
// ("slice bounds out of range [:551] with capacity 550"). v0.3.0 is the newest
// release, so there is no upgrade that fixes it. Left unprotected, one snippet
// out of a repository takes the whole process down: the daemon's embedding
// module crash-looped on this for twelve days.
func (c *localEmbeddingClient) encodeSingle(text string) (ids, mask []int, err error) {
	defer func() {
		if r := recover(); r != nil {
			ids, mask, err = nil, nil, fmt.Errorf("tokenizer panic: %v", r)
		}
	}()
	enc, encErr := c.tk.EncodeSingle(text, true)
	if encErr != nil {
		return nil, nil, encErr
	}
	return enc.GetIds(), enc.GetAttentionMask(), nil
}

func (c *localEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	batchSize := len(texts)

	allIDs := make([][]int, batchSize)
	allMasks := make([][]int, batchSize)
	maxLen := 0

	// A text whose tokenization failed. Its tensor row stays all-padding and its
	// result is returned as nil, which callers already treat as "no vector for
	// this one" — so one unencodable snippet costs its own embedding and nothing
	// else in the batch.
	failed := make([]bool, batchSize)
	encodable := 0

	for i, text := range texts {
		ids, mask, err := c.encodeSingle(text)
		if err != nil {
			failed[i] = true
			continue
		}

		if len(mask) > len(ids) {
			mask = mask[:len(ids)]
		}
		if len(ids) > maxSeqLen {
			ids = ids[:maxSeqLen]
			if len(mask) > maxSeqLen {
				mask = mask[:maxSeqLen]
			}
		}

		allIDs[i] = ids
		allMasks[i] = mask
		encodable++

		if len(ids) > maxLen {
			maxLen = len(ids)
		}
	}

	// Every text failed, or every one encoded empty: there is no tensor to build
	// — a shape with a zero dimension is not something to hand the model.
	if encodable == 0 || maxLen == 0 {
		return make([][]float32, batchSize), nil
	}

	inputIDs := make([]int64, batchSize*maxLen)
	attentionMask := make([]int64, batchSize*maxLen)

	for i := 0; i < batchSize; i++ {
		for j := 0; j < len(allIDs[i]) && j < len(allMasks[i]); j++ {
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
		if failed[i] {
			continue
		}
		lo, hi := i*hiddenDim, (i+1)*hiddenDim
		if hi > len(outputData) {
			break
		}
		vec := make([]float32, hiddenDim)
		copy(vec, outputData[lo:hi])

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
