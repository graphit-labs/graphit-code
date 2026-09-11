package ai

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/sugarme/tokenizer/pretrained"
	ort "github.com/yalue/onnxruntime_go"

	tokenizer "github.com/sugarme/tokenizer"
)

type textEncoder interface {
	EncodeSingle(string, ...bool) (*tokenizer.Encoding, error)
}

type embeddingSession interface {
	Run(inputs, outputs []ort.Value) error
	Destroy() error
}

type embeddingSessionFactory func(ONNXExecutionConfig) (embeddingSession, ONNXDevice, error)

const (
	embeddingAcceleratorRetryInitial = time.Minute
	embeddingAcceleratorRetryMax     = 10 * time.Minute
)

type localEmbeddingClient struct {
	tk      textEncoder
	session embeddingSession

	modelName      string
	dimensions     int
	maxTokens      int
	queryPrefix    string
	documentPrefix string
	pooling        string
	normalize      bool
	inputs         []onnxInputBinding
	output         string
	execution      ONNXExecutionConfig
	device         ONNXDevice
	accelerator    ONNXDevice
	newSession     embeddingSessionFactory
	now            func() time.Time
	retryAt        time.Time
	retryDelay     time.Duration

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
	var libNames []string
	switch runtime.GOOS {
	case "windows":
		libNames = []string{"onnxruntime.dll"}
	case "darwin":
		libNames = []string{"libonnxruntime.1.29.0.dylib", "libonnxruntime.dylib"}
	default:
		libNames = []string{"libonnxruntime.so.1.29.0", "libonnxruntime.so"}
	}

	if exe, err := os.Executable(); err == nil {
		if candidate := firstExistingLibrary(filepath.Dir(exe), libNames); candidate != "" {
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
		if candidate := firstExistingLibrary(d, libNames); candidate != "" {
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
			if candidate := firstExistingLibrary(dir, libNames); candidate != "" {
				return candidate
			}
		}
	}

	return ""
}

func firstExistingLibrary(dir string, names []string) string {
	for _, name := range names {
		candidate := filepath.Join(dir, name)
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			return candidate
		}
	}
	return ""
}

func NewLocalEmbeddingClient() (*localEmbeddingClient, error) {
	execution, err := configuredONNXExecution(ModelTaskEmbedding)
	if err != nil {
		return nil, err
	}
	if err := initONNXRuntime(); err != nil {
		return nil, fmt.Errorf("init ONNX Runtime: %w", err)
	}
	model, err := LoadConfiguredModel(ModelTaskEmbedding)
	if err != nil {
		return nil, fmt.Errorf("resolve local embedding model: %w", err)
	}
	paths, err := model.Ensure(context.Background())
	if err != nil {
		return nil, fmt.Errorf("ensure local embedding model: %w", err)
	}
	modelRole := model.Manifest.Runtime.Entrypoints["model"]
	modelPath := paths[modelRole]
	tokenizerPath := paths[model.Manifest.Tokenizer.Artifact]
	return newLocalEmbeddingClient(model, modelPath, tokenizerPath, execution)
}

func newLocalEmbeddingClient(model *ResolvedModel, modelPath, tokenizerPath string, execution ONNXExecutionConfig) (*localEmbeddingClient, error) {
	if err := initONNXRuntime(); err != nil {
		return nil, fmt.Errorf("init ONNX Runtime: %w", err)
	}

	tk, err := pretrained.FromFile(tokenizerPath)
	if err != nil {
		return nil, fmt.Errorf("load tokenizer from %s: %w", tokenizerPath, err)
	}

	contract, err := inspectONNXTextContract(model.Manifest, modelPath)
	if err != nil {
		return nil, err
	}
	inputNames := make([]string, len(contract.Inputs))
	for i, input := range contract.Inputs {
		inputNames[i] = input.Name
	}

	newSession := func(candidate ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
		return newDynamicONNXSession(
			modelPath,
			inputNames,
			[]string{contract.Output},
			candidate,
			func(opts *ort.SessionOptions) error {
				if err := opts.SetIntraOpNumThreads(boundedEmbedThreads()); err != nil {
					return fmt.Errorf("set embedding intra-op threads: %w", err)
				}
				if err := opts.SetInterOpNumThreads(1); err != nil {
					return fmt.Errorf("set embedding inter-op threads: %w", err)
				}
				return nil
			},
		)
	}
	session, selected, err := newSession(execution)
	if err != nil {
		return nil, fmt.Errorf("create embedding ONNX session: %w", err)
	}

	client := &localEmbeddingClient{
		tk:             tk,
		session:        session,
		modelName:      model.Manifest.Name,
		dimensions:     model.Manifest.Inference.Dimensions,
		maxTokens:      model.Manifest.Text.MaxTokens,
		queryPrefix:    model.Manifest.Text.QueryPrefix,
		documentPrefix: model.Manifest.Text.DocumentPrefix,
		pooling:        model.Manifest.Inference.Pooling,
		normalize:      model.Manifest.Inference.Normalize != nil && *model.Manifest.Inference.Normalize,
		inputs:         contract.Inputs,
		output:         contract.Output,
		execution:      execution,
		device:         selected,
		accelerator:    autoAcceleratorDevice(runtime.GOOS),
		newSession:     newSession,
		now:            time.Now,
	}
	if execution.Device == ONNXDeviceAuto && selected == ONNXDeviceCPU {
		client.scheduleAcceleratorRetryLocked()
	}
	return client, nil
}

func boundedEmbedThreads() int {
	if s := os.Getenv("GRAPHIT_EMBED_THREADS"); s != "" {
		if n, err := strconv.Atoi(s); err == nil && n > 0 {
			return n
		}
	}
	return sysutil.CPUBudget()
}

func (c *localEmbeddingClient) ModelName() string { return c.modelName }

func (c *localEmbeddingClient) Dimensions() int { return c.dimensions }

func (c *localEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	vecs, err := c.embedBatch(ctx, []string{text}, c.documentPrefix)
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("empty embedding response")
	}
	return vecs[0], nil
}

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
	return c.embedBatch(ctx, texts, c.documentPrefix)
}

func (c *localEmbeddingClient) embedBatch(ctx context.Context, texts []string, prefix string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	batchSize := len(texts)

	allIDs := make([][]int, batchSize)
	allMasks := make([][]int, batchSize)
	maxLen := 0

	failed := make([]bool, batchSize)
	encodable := 0

	for i, text := range texts {
		ids, mask, err := c.encodeSingle(prefix + text)
		if err != nil {
			failed[i] = true
			continue
		}

		if len(mask) > len(ids) {
			mask = mask[:len(ids)]
		}
		if len(ids) > c.maxTokens {
			ids = ids[:c.maxTokens]
			if len(mask) > c.maxTokens {
				mask = mask[:c.maxTokens]
			}
		}

		allIDs[i] = ids
		allMasks[i] = mask
		encodable++

		if len(ids) > maxLen {
			maxLen = len(ids)
		}
	}

	if encodable == 0 || maxLen == 0 {
		return make([][]float32, batchSize), nil
	}

	inputIDs := make([]int64, batchSize*maxLen)
	attentionMask := make([]int64, batchSize*maxLen)
	tokenTypeIDs := make([]int64, batchSize*maxLen)

	for i := 0; i < batchSize; i++ {
		for j := 0; j < len(allIDs[i]) && j < len(allMasks[i]); j++ {
			idx := i*maxLen + j
			inputIDs[idx] = int64(allIDs[i][j])
			attentionMask[idx] = int64(allMasks[i][j])
		}

	}

	shape := ort.Shape{int64(batchSize), int64(maxLen)}
	byRole := map[string][]int64{
		"input_ids":      inputIDs,
		"attention_mask": attentionMask,
		"token_type_ids": tokenTypeIDs,
	}
	inputs := make([]ort.Value, 0, len(c.inputs))
	defer func() {
		for _, input := range inputs {
			_ = input.Destroy()
		}
	}()
	for _, binding := range c.inputs {
		data, ok := byRole[binding.Role]
		if !ok {
			return nil, fmt.Errorf("unsupported embedding input role %q", binding.Role)
		}
		tensor, tensorErr := ort.NewTensor(shape, data)
		if tensorErr != nil {
			return nil, fmt.Errorf("create embedding input %s (%s): %w", binding.Name, binding.Role, tensorErr)
		}
		inputs = append(inputs, tensor)
	}

	outputs := []ort.Value{nil}
	err := c.runONNX(inputs, outputs)
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
	outputShape := outputTensor.GetShape()

	results := make([][]float32, batchSize)
	for i := 0; i < batchSize; i++ {
		if failed[i] {
			continue
		}
		vec, poolErr := c.poolEmbedding(outputData, outputShape, i, batchSize, allMasks[i])
		if poolErr != nil {
			return nil, poolErr
		}
		results[i] = vec
	}

	return results, nil
}

func autoAcceleratorDevice(goos string) ONNXDevice {
	if goos == "darwin" {
		return ONNXDeviceCoreML
	}
	return ONNXDeviceCUDA
}

func (c *localEmbeddingClient) runONNX(inputs, outputs []ort.Value) error {
	c.mu.Lock()
	defer c.mu.Unlock()

	if c.session == nil {
		return fmt.Errorf("embedding ONNX session is closed")
	}
	if c.shouldProbeAcceleratorLocked() && c.tryAcceleratorLocked(inputs, outputs) {
		return nil
	}

	err := c.session.Run(inputs, outputs)
	if err == nil {
		return nil
	}
	clearONNXOutputs(outputs)
	if c.execution.Device != ONNXDeviceAuto || c.device == ONNXDeviceCPU || !isAcceleratorResourceError(err) {
		return err
	}
	return c.fallbackToCPULocked(inputs, outputs, err)
}

func (c *localEmbeddingClient) shouldProbeAcceleratorLocked() bool {
	if c.execution.Device != ONNXDeviceAuto || c.device != ONNXDeviceCPU || c.newSession == nil || c.accelerator == "" {
		return false
	}
	now := c.currentTime()
	return c.retryAt.IsZero() || !now.Before(c.retryAt)
}

func (c *localEmbeddingClient) tryAcceleratorLocked(inputs, outputs []ort.Value) bool {
	candidate, selected, err := c.newSession(ONNXExecutionConfig{Device: c.accelerator, DeviceID: c.execution.DeviceID})
	if err != nil {
		c.scheduleAcceleratorRetryLocked()
		slog.Warn("embedding accelerator probe failed; continuing on CPU",
			"accelerator", c.accelerator, "device_id", c.execution.DeviceID,
			"retry_at", c.retryAt, "error", err)
		return false
	}
	if candidate == nil {
		c.scheduleAcceleratorRetryLocked()
		slog.Warn("embedding accelerator probe returned no session; continuing on CPU",
			"accelerator", c.accelerator, "device_id", c.execution.DeviceID,
			"retry_at", c.retryAt)
		return false
	}
	if selected != c.accelerator {
		_ = candidate.Destroy()
		c.scheduleAcceleratorRetryLocked()
		slog.Warn("embedding accelerator probe selected an unexpected device; continuing on CPU",
			"accelerator", c.accelerator, "selected", selected, "retry_at", c.retryAt)
		return false
	}
	if err := candidate.Run(inputs, outputs); err != nil {
		clearONNXOutputs(outputs)
		_ = candidate.Destroy()
		c.scheduleAcceleratorRetryLocked()
		slog.Warn("embedding accelerator probe inference failed; continuing on CPU",
			"accelerator", c.accelerator, "device_id", c.execution.DeviceID,
			"retry_at", c.retryAt, "error", err)
		return false
	}

	previous := c.session
	c.session = candidate
	c.device = selected
	c.resetAcceleratorRetryLocked()
	_ = previous.Destroy()
	slog.Info("embedding execution returned to accelerator",
		"accelerator", selected, "device_id", c.execution.DeviceID)
	return true
}

func (c *localEmbeddingClient) fallbackToCPULocked(inputs, outputs []ort.Value, acceleratorErr error) error {
	if c.newSession == nil {
		return acceleratorErr
	}
	cpu, selected, err := c.newSession(ONNXExecutionConfig{Device: ONNXDeviceCPU, DeviceID: 0})
	if err != nil {
		return acceleratorFallbackError(acceleratorErr, fmt.Errorf("create CPU fallback: %w", err))
	}
	if cpu == nil {
		return acceleratorFallbackError(acceleratorErr, errors.New("CPU fallback returned no session"))
	}
	if selected != ONNXDeviceCPU {
		_ = cpu.Destroy()
		return acceleratorFallbackError(acceleratorErr, fmt.Errorf("CPU fallback selected unexpected device %q", selected))
	}
	if err := cpu.Run(inputs, outputs); err != nil {
		clearONNXOutputs(outputs)
		_ = cpu.Destroy()
		return acceleratorFallbackError(acceleratorErr, fmt.Errorf("CPU fallback inference: %w", err))
	}

	previous := c.session
	c.session = cpu
	c.device = ONNXDeviceCPU
	c.retryDelay = 0
	c.scheduleAcceleratorRetryLocked()
	_ = previous.Destroy()
	slog.Warn("embedding accelerator exhausted resources; continuing on CPU",
		"accelerator", c.accelerator, "device_id", c.execution.DeviceID,
		"retry_at", c.retryAt, "error", acceleratorErr)
	return nil
}

func acceleratorFallbackError(acceleratorErr, fallbackErr error) error {
	return errors.Join(
		fmt.Errorf("accelerator inference: %w", acceleratorErr),
		fallbackErr,
	)
}

func (c *localEmbeddingClient) currentTime() time.Time {
	if c.now != nil {
		return c.now()
	}
	return time.Now()
}

func (c *localEmbeddingClient) scheduleAcceleratorRetryLocked() {
	if c.retryDelay <= 0 {
		c.retryDelay = embeddingAcceleratorRetryInitial
	} else {
		c.retryDelay *= 2
		if c.retryDelay > embeddingAcceleratorRetryMax {
			c.retryDelay = embeddingAcceleratorRetryMax
		}
	}
	c.retryAt = c.currentTime().Add(c.retryDelay)
}

func (c *localEmbeddingClient) resetAcceleratorRetryLocked() {
	c.retryAt = time.Time{}
	c.retryDelay = 0
}

func clearONNXOutputs(outputs []ort.Value) {
	for i, output := range outputs {
		if output != nil {
			_ = output.Destroy()
			outputs[i] = nil
		}
	}
}

func isAcceleratorResourceError(err error) bool {
	if err == nil {
		return false
	}
	message := strings.ToLower(err.Error())
	for _, marker := range []string{
		"out of memory",
		"failed to allocate memory",
		"resource allocation failed",
		"cuda_error_out_of_memory",
		"cudaerrormemoryallocation",
		"cublas_status_alloc_failed",
		"cudnn_status_alloc_failed",
		"bfc_arena.cc",
	} {
		if strings.Contains(message, marker) {
			return true
		}
	}
	return false
}

func (c *localEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	vecs, err := c.embedBatch(ctx, []string{query}, c.queryPrefix)
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("empty embedding response")
	}
	return vecs[0], nil
}

func (c *localEmbeddingClient) poolEmbedding(data []float32, shape ort.Shape, batchIndex, batchSize int, mask []int) ([]float32, error) {
	if c.dimensions <= 0 {
		return nil, fmt.Errorf("embedding dimensions must be positive")
	}
	var vector []float32
	switch c.pooling {
	case "none":
		expected := batchSize * c.dimensions
		if len(shape) != 2 || shape[0] != int64(batchSize) || shape[1] != int64(c.dimensions) || len(data) != expected {
			return nil, fmt.Errorf("embedding output %q has shape %v and %d values, want [%d,%d]", c.output, shape, len(data), batchSize, c.dimensions)
		}
		start := batchIndex * c.dimensions
		vector = append([]float32(nil), data[start:start+c.dimensions]...)
	case "mean", "cls":
		if len(shape) != 3 || batchSize <= 0 || shape[0] != int64(batchSize) || shape[2] != int64(c.dimensions) || shape[1] <= 0 || len(data) != batchSize*int(shape[1])*c.dimensions {
			return nil, fmt.Errorf("embedding output %q has incompatible token tensor shape %v", c.output, shape)
		}
		sequenceLength := len(data) / (batchSize * c.dimensions)
		base := batchIndex * sequenceLength * c.dimensions
		vector = make([]float32, c.dimensions)
		if c.pooling == "cls" {
			copy(vector, data[base:base+c.dimensions])
			break
		}
		count := 0
		for token := 0; token < sequenceLength && token < len(mask); token++ {
			if mask[token] == 0 {
				continue
			}
			start := base + token*c.dimensions
			for dimension := range vector {
				vector[dimension] += data[start+dimension]
			}
			count++
		}
		if count == 0 {
			return nil, fmt.Errorf("embedding output %q has no unmasked tokens to mean-pool", c.output)
		}
		for dimension := range vector {
			vector[dimension] /= float32(count)
		}
	default:
		return nil, fmt.Errorf("unsupported embedding pooling %q", c.pooling)
	}
	if c.normalize {
		var norm float64
		for _, value := range vector {
			norm += float64(value) * float64(value)
		}
		norm = math.Sqrt(norm)
		if norm > 0 {
			for i := range vector {
				vector[i] = float32(float64(vector[i]) / norm)
			}
		}
	}
	return vector, nil
}

func (c *localEmbeddingClient) Close() {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.session == nil {
		return
	}
	_ = c.session.Destroy()
	c.session = nil
}
