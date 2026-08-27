package ai

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

const (
	modelCacheSubdir = "models/coderankembed"

	modelONNXURL     = "https://huggingface.co/mrsladoje/CodeRankEmbed-onnx-int8/resolve/main/onnx/model.onnx"
	tokenizerJSONURL = "https://huggingface.co/mrsladoje/CodeRankEmbed-onnx-int8/resolve/main/tokenizer.json"

	modelONNXMinSize     = 100_000_000
	tokenizerJSONMinSize = 500_000

	modelFileName = "model.onnx"

	tokenizerFileName = "tokenizer.json"
)

// ProgressFunc is called as a file of the model bundle arrives. file is the
// destination's base name (model.onnx, tokenizer.json), and total is the size
// the server declared — zero when it declared none.
//
// It is called once with downloaded == 0 before the first byte, so a caller can
// announce the download before there is anything to report, and then once per
// read after that. Reads are 32 KB, so a 132 MB model produces some four
// thousand calls: throttling is the caller's job, because how often a line can
// be repainted is a property of where it is going, not of the download.
type ProgressFunc func(file string, downloaded, total int64)

type ModelManager struct {
	Logger   *slog.Logger
	cacheDir string

	// OnProgress, when set, reports download progress. Nil means the download
	// runs silently, which is what every non-interactive caller wants.
	OnProgress ProgressFunc

	// Where the model and tokenizer are fetched from. Empty means the shipped
	// constants above, which is every production path — NewModelManager does not
	// set these. They exist so a test can point EnsureModel at a local server:
	// without them the only way to exercise the download branch was to let it
	// reach huggingface.co and move 132 MB, which made the suite depend on a
	// third party being reachable and cost more wall-clock than every other test
	// in this package combined.
	modelURL     string
	tokenizerURL string
}

func (m *ModelManager) modelSource() string {
	if m.modelURL != "" {
		return m.modelURL
	}
	return modelONNXURL
}

func (m *ModelManager) tokenizerSource() string {
	if m.tokenizerURL != "" {
		return m.tokenizerURL
	}
	return tokenizerJSONURL
}

func (m *ModelManager) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

// ModelCacheDir is where the model bundle lives: one copy per machine, shared by
// every project and every installed version.
//
// It is exported because a caller that has to explain a failed download needs to
// name the directory, and a failure is precisely the case where EnsureModel
// returns no path to derive it from.
// ModelsDir is the directory that holds ONE SUBDIRECTORY PER MODEL — the retrieval embedder, the
// reranker, and anything added later.
//
// <BRAND>_MODEL_CACHE overrides it, and the reason is a cost that was invisible: these models are
// large, the path is derived from HOME, and every test binary gets its OWN throwaway HOME (see
// internal/brand/testhome.go). So a test run downloaded the ~132 MB embedding model once PER
// PACKAGE and left each copy behind — measured on this machine, 29 abandoned homes holding 4.3 GB,
// on a tmpfs, which is RAM. The override points every one of them at a single shared directory;
// `make test` does exactly that.
//
// It is not only a test knob. These are by far the largest things this framework stores — the
// reranker alone is ~1.1 GiB — and an operator whose home is on a small volume has no other way to
// move them.
//
// THE OVERRIDE NAMES THE ROOT, not one model's directory, and that is load-bearing: the reranker
// resolves its own path from this same root, so overriding a leaf would move one model and leave
// the other behind in the real home.
func ModelsDir() (string, error) {
	if d := os.Getenv(brand.EnvVar("MODEL_CACHE")); d != "" {
		return d, nil
	}
	global := brand.GlobalDir()
	if global == "" {
		return "", fmt.Errorf("home dir: cannot resolve the global %s directory", brand.Brand)
	}
	return filepath.Join(global, filepath.Dir(modelCacheSubdir)), nil
}

// ModelCacheDir is where the retrieval embedding model is kept.
func ModelCacheDir() (string, error) {
	root, err := ModelsDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(root, filepath.Base(modelCacheSubdir)), nil
}

func NewModelManager() (*ModelManager, error) {
	cacheDir, err := ModelCacheDir()
	if err != nil {
		return nil, err
	}
	return &ModelManager{cacheDir: cacheDir}, nil
}

func (m *ModelManager) EnsureModel(ctx context.Context) (modelPath, tokenizerPath string, err error) {

	if bundledDir := m.findBundledModels(); bundledDir != "" {
		bundledModel := filepath.Join(bundledDir, modelFileName)
		bundledTokenizer := filepath.Join(bundledDir, tokenizerFileName)
		if m.isValid(bundledModel, modelONNXMinSize) && m.isValid(bundledTokenizer, tokenizerJSONMinSize) {
			return bundledModel, bundledTokenizer, nil
		}
	}

	cachedModel := filepath.Join(m.cacheDir, modelFileName)
	cachedTokenizer := filepath.Join(m.cacheDir, tokenizerFileName)
	if m.isValid(cachedModel, modelONNXMinSize) && m.isValid(cachedTokenizer, tokenizerJSONMinSize) {
		return cachedModel, cachedTokenizer, nil
	}

	if err := os.MkdirAll(m.cacheDir, 0o755); err != nil {
		return "", "", fmt.Errorf("create model cache dir: %w", err)
	}

	if !m.isValid(cachedModel, modelONNXMinSize) {
		m.log().Info("downloading model", "model", "CodeRankEmbed-137M-INT8", "size", "~132MB")
		if err := m.download(ctx, m.modelSource(), cachedModel); err != nil {
			return "", "", fmt.Errorf("download model: %w", err)
		}
		if !m.isValid(cachedModel, modelONNXMinSize) {
			return "", "", fmt.Errorf("downloaded model too small — expected at least %d bytes", modelONNXMinSize)
		}
		m.log().Info("model download complete")
	}

	if !m.isValid(cachedTokenizer, tokenizerJSONMinSize) {
		m.log().Info("downloading tokenizer")
		if err := m.download(ctx, m.tokenizerSource(), cachedTokenizer); err != nil {
			return "", "", fmt.Errorf("download tokenizer: %w", err)
		}
		if !m.isValid(cachedTokenizer, tokenizerJSONMinSize) {
			return "", "", fmt.Errorf("downloaded tokenizer too small — expected at least %d bytes", tokenizerJSONMinSize)
		}
	}

	return cachedModel, cachedTokenizer, nil
}

func (m *ModelManager) isValid(path string, minSize int64) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return info.Size() >= minSize
}

func (m *ModelManager) download(ctx context.Context, url, destPath string) error {
	client := &http.Client{}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}

	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("HTTP GET %s: %w", url, err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP %d from %s", resp.StatusCode, url)
	}

	tmpPath := destPath + ".tmp"
	f, err := os.Create(tmpPath)
	if err != nil {
		return err
	}

	// The name reported is the final one, not the .tmp the bytes land in —
	// the caller is showing this to a person who asked for a model.
	var dst io.Writer = f
	if m.OnProgress != nil {
		name := filepath.Base(destPath)
		m.OnProgress(name, 0, resp.ContentLength)
		dst = &progressWriter{
			w:      f,
			file:   name,
			total:  resp.ContentLength,
			report: m.OnProgress,
		}
	}

	written, err := io.Copy(dst, resp.Body)
	_ = f.Close()
	if err != nil {
		_ = os.Remove(tmpPath)
		return fmt.Errorf("write: %w", err)
	}

	if resp.ContentLength > 0 && written != resp.ContentLength {
		_ = os.Remove(tmpPath)
		return fmt.Errorf("incomplete download: wrote %d of %d bytes", written, resp.ContentLength)
	}

	if err := os.Rename(tmpPath, destPath); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}

	return nil
}

// progressWriter counts bytes on their way to the file and reports each write.
// It deliberately does not throttle: see ProgressFunc.
type progressWriter struct {
	w          io.Writer
	file       string
	total      int64
	downloaded int64
	report     ProgressFunc
}

func (pw *progressWriter) Write(p []byte) (int, error) {
	n, err := pw.w.Write(p)
	pw.downloaded += int64(n)
	pw.report(pw.file, pw.downloaded, pw.total)
	return n, err
}

// findBundledModels looks for a model shipped next to the executable. The
// released binaries do not carry one — the model is downloaded at setup into
// the shared cache — but a private build that prefers to ship it, in an air
// gapped environment for instance, is served by dropping a models/ directory
// beside the core binary, and it wins over the cache.
func (m *ModelManager) findBundledModels() string {
	exe, err := os.Executable()
	if err != nil {
		return ""
	}
	modelsDir := filepath.Join(filepath.Dir(exe), "models")
	if info, err := os.Stat(modelsDir); err == nil && info.IsDir() {
		return modelsDir
	}
	return ""
}
