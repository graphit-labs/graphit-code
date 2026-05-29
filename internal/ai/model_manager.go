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

type ModelManager struct {
	Logger   *slog.Logger
	cacheDir string
}

func (m *ModelManager) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

func NewModelManager() (*ModelManager, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, fmt.Errorf("home dir: %w", err)
	}

	cacheDir := filepath.Join(home, brand.DotDir(), modelCacheSubdir)
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
		if err := m.download(ctx, modelONNXURL, cachedModel); err != nil {
			return "", "", fmt.Errorf("download model: %w", err)
		}
		if !m.isValid(cachedModel, modelONNXMinSize) {
			return "", "", fmt.Errorf("downloaded model too small — expected at least %d bytes", modelONNXMinSize)
		}
		m.log().Info("model download complete")
	}

	if !m.isValid(cachedTokenizer, tokenizerJSONMinSize) {
		m.log().Info("downloading tokenizer")
		if err := m.download(ctx, tokenizerJSONURL, cachedTokenizer); err != nil {
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

	written, err := io.Copy(f, resp.Body)
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
