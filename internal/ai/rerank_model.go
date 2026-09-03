package ai

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

const (
	rerankCacheSubdir = "models/bge-reranker-base"

	rerankModelName    = "bge-reranker-base"
	rerankModelURL     = "https://huggingface.co/BAAI/bge-reranker-base/resolve/main/onnx/model.onnx"
	rerankTokenizerURL = "https://huggingface.co/BAAI/bge-reranker-base/resolve/main/tokenizer.json"

	rerankModelMinSize     = 900_000_000
	rerankTokenizerMinSize = 5_000_000
)

// RerankModelManager resolves the reranker's model and tokenizer, downloading them only when
// asked.
//
// It is deliberately a separate type from ModelManager rather than a mode of it: the two differ
// in the one thing that matters here, which is WHEN they are allowed to reach the network.
// ModelManager is called by `setup` and by the indexing path; this one is called only after a
// caller has opted into reranking.
type RerankModelManager struct {
	Logger *slog.Logger

	cacheDir string

	// OnProgress, when set, reports download progress. Nil downloads silently.
	OnProgress ProgressFunc
}

func (m *RerankModelManager) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

// NewRerankModelManager builds the manager. It touches no network and creates no directory:
// constructing one is free, and only Ensure can download.
func NewRerankModelManager() (*RerankModelManager, error) {
	root, err := ModelsDir()
	if err != nil {
		return nil, err
	}
	return &RerankModelManager{cacheDir: filepath.Join(root, filepath.Base(rerankCacheSubdir))}, nil
}

// CacheDir is where the bundle lives.
func (m *RerankModelManager) CacheDir() string { return m.cacheDir }

// Present reports whether the bundle is already on disk, WITHOUT downloading anything.
//
// This is what lets a caller decide before committing: a UI can say "reranking needs a 1 GiB
// model, fetch it?" instead of blocking on a download the user did not expect.
func (m *RerankModelManager) Present() bool {
	return isValidFile(filepath.Join(m.cacheDir, modelFileName), rerankModelMinSize) &&
		isValidFile(filepath.Join(m.cacheDir, tokenizerFileName), rerankTokenizerMinSize)
}

// Ensure returns the paths to the model and tokenizer, downloading them if they are absent.
//
// CALLING THIS IS THE COMMITMENT. Everything above it in the stack is gated so that it is only
// reached when reranking was explicitly enabled — see ai.NewCrossEncoderReranker and the
// search.rerank configuration key.
func (m *RerankModelManager) Ensure(ctx context.Context) (modelPath, tokenizerPath string, err error) {
	modelPath = filepath.Join(m.cacheDir, modelFileName)
	tokenizerPath = filepath.Join(m.cacheDir, tokenizerFileName)

	if m.Present() {
		return modelPath, tokenizerPath, nil
	}

	if err := os.MkdirAll(m.cacheDir, 0o755); err != nil {
		return "", "", fmt.Errorf("create reranker cache dir: %w", err)
	}

	dl := &ModelManager{Logger: m.Logger, cacheDir: m.cacheDir, OnProgress: m.OnProgress}

	if !isValidFile(modelPath, rerankModelMinSize) {
		m.log().Info("downloading reranker model",
			"model", rerankModelName, "size", "~1.04GiB",
			"why", "reranking was enabled and the model is not present")
		if err := dl.download(ctx, rerankModelURL, modelPath); err != nil {
			return "", "", fmt.Errorf("download reranker model: %w", err)
		}
		if !isValidFile(modelPath, rerankModelMinSize) {
			return "", "", fmt.Errorf("downloaded reranker model is too small — expected at least %d bytes",
				rerankModelMinSize)
		}
	}

	if !isValidFile(tokenizerPath, rerankTokenizerMinSize) {
		if err := dl.download(ctx, rerankTokenizerURL, tokenizerPath); err != nil {
			return "", "", fmt.Errorf("download reranker tokenizer: %w", err)
		}
		if !isValidFile(tokenizerPath, rerankTokenizerMinSize) {
			return "", "", fmt.Errorf("downloaded reranker tokenizer is too small — expected at least %d bytes",
				rerankTokenizerMinSize)
		}
	}

	m.log().Info("reranker model ready", "dir", m.cacheDir)
	return modelPath, tokenizerPath, nil
}

func isValidFile(path string, minSize int64) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir() && info.Size() >= minSize
}
