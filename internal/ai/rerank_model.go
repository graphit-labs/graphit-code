package ai

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// The cross-encoder reranker's model bundle, and the rule that governs it.
//
// THE MODEL IS NEVER FETCHED UNLESS SOMEBODY ASKED FOR RERANKING AND IT IS NOT ALREADY THERE.
// That is not an optimisation. It is 1.04 GiB — eight times the retrieval embedder — and reranking
// is opt-in and off by default, so a user who never turns it on must never pay for it: not at
// `setup`, not on first query, not at all. `graphit setup` does not touch this manager, and
// nothing constructs a reranker client eagerly.
//
// WHY THIS MODEL, AND WHY NOT THE OBVIOUS ONE.
//
// The first choice was jina-reranker-v2-base-multilingual, on the strength of being the only small
// reranker with published code-retrieval benchmarks. It is licensed cc-by-nc-4.0 — NON-COMMERCIAL.
// That disqualifies it here regardless of how well it scores, and a benchmark table is not a
// licence review: the two candidates left were then MEASURED rather than argued about, with real
// inference over real entities of this repository (internal/ai/rerank_eval_test.go, `rerankeval`).
//
//	model                            licence      size     MRR             nDCG@10         per query
//	bge-reranker-base                MIT          1.04 GiB 0.833 -> 0.865  0.860 -> 0.883  720ms
//	ms-marco-MiniLM-L-6-v2           Apache-2.0   87 MiB   0.833 -> 0.828  0.860 -> 0.856  92ms
//
// ms-marco is a tenth of the size and eight times faster AND IT MADE THE RANKING WORSE, which is
// the whole reason this table exists instead of a paragraph reasoning from parameter counts. It is
// trained on natural-language passages; an identifier with a docstring is not a passage.
//
// bge-reranker-base is XLM-RoBERTa, so it has no `token_type_ids` input — see newCrossEncoderFrom,
// which discovers the input set from the model rather than assuming BERT's three.
//
// There is no quantised ONNX upstream: onnx/model_quantized.onnx is a 404, not a small file.

const (
	rerankCacheSubdir = "models/bge-reranker-base"

	rerankModelName    = "bge-reranker-base"
	rerankModelURL     = "https://huggingface.co/BAAI/bge-reranker-base/resolve/main/onnx/model.onnx"
	rerankTokenizerURL = "https://huggingface.co/BAAI/bge-reranker-base/resolve/main/tokenizer.json"

	// Size floors, so a truncated or error-page download is caught here rather than as a corrupt
	// session three layers away. MEASURED upstream: the model is 1,112,459,588 bytes and the
	// tokenizer 17,098,107. The floors sit well below both so that a legitimate re-export upstream
	// does not fail the check, and well above an HTML error page.
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

	// Sources, empty in production. They exist so a test can point Ensure at a local server
	// instead of moving a gigabyte from a third party — the same reason ModelManager has them.
	modelURL     string
	tokenizerURL string
}

func (m *RerankModelManager) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

// NewRerankModelManager builds the manager. It touches no network and creates no directory:
// constructing one is free, and only Ensure can download.
func NewRerankModelManager() (*RerankModelManager, error) {
	root, err := ModelsDir()
	if err != nil {
		return nil, err
	}
	// The reranker sits BESIDE the retrieval model under its own name, so the two are never
	// confused by a cleanup that removes one.
	//
	// Resolved from the models root rather than from ModelCacheDir's parent, which is what it
	// used to do. That derivation broke the moment the root became overridable: taking the parent
	// of an overridden leaf lands wherever that leaf happens to sit — measured, an override of
	// /tmp/graphit-model-cache put the reranker in /tmp/bge-reranker-base, outside any directory
	// this framework owns.
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
		if err := dl.download(ctx, m.modelSource(), modelPath); err != nil {
			return "", "", fmt.Errorf("download reranker model: %w", err)
		}
		if !isValidFile(modelPath, rerankModelMinSize) {
			return "", "", fmt.Errorf("downloaded reranker model is too small — expected at least %d bytes",
				rerankModelMinSize)
		}
	}

	if !isValidFile(tokenizerPath, rerankTokenizerMinSize) {
		if err := dl.download(ctx, m.tokenizerSourceURL(), tokenizerPath); err != nil {
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

func (m *RerankModelManager) modelSource() string {
	if m.modelURL != "" {
		return m.modelURL
	}
	return rerankModelURL
}

func (m *RerankModelManager) tokenizerSourceURL() string {
	if m.tokenizerURL != "" {
		return m.tokenizerURL
	}
	return rerankTokenizerURL
}

// isValidFile is the shared size check. A download that returned an HTML error page is a few
// hundred bytes, and a session built on it fails with a message that names nothing useful.
func isValidFile(path string, minSize int64) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir() && info.Size() >= minSize
}
