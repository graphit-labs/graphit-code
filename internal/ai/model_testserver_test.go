package ai

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func sparseFile(t *testing.T, path string, size int64) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir for %s: %v", path, err)
	}
	f, err := os.Create(path)
	if err != nil {
		t.Fatalf("create %s: %v", path, err)
	}
	defer func() { _ = f.Close() }()
	if err := f.Truncate(size); err != nil {
		t.Fatalf("truncate %s to %d: %v", path, size, err)
	}
}

func modelServer(t *testing.T, cacheDir string) *ModelManager {
	t.Helper()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var size int64
		switch {
		case strings.Contains(r.URL.Path, modelFileName):
			size = modelONNXMinSize + 1
		case strings.Contains(r.URL.Path, tokenizerFileName):
			size = tokenizerJSONMinSize + 1
		default:
			w.WriteHeader(http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Length", strconv.FormatInt(size, 10))
		w.WriteHeader(http.StatusOK)
		_, _ = writeZeros(w, size)
	}))
	t.Cleanup(srv.Close)

	return &ModelManager{
		cacheDir:     cacheDir,
		modelURL:     srv.URL + "/" + modelFileName,
		tokenizerURL: srv.URL + "/" + tokenizerFileName,
	}
}

func writeZeros(w http.ResponseWriter, n int64) (int64, error) {
	const chunk = 1 << 20
	buf := make([]byte, chunk)
	var written int64
	for written < n {
		size := int64(chunk)
		if remaining := n - written; remaining < size {
			size = remaining
		}
		got, err := w.Write(buf[:size])
		written += int64(got)
		if err != nil {
			return written, err
		}
	}
	return written, nil
}

func seedModelCache(t *testing.T, home string) {
	t.Helper()
	dir := filepath.Join(home, brand.DotDir(), modelCacheSubdir)
	sparseFile(t, filepath.Join(dir, modelFileName), modelONNXMinSize+1)
	sparseFile(t, filepath.Join(dir, tokenizerFileName), tokenizerJSONMinSize+1)
}
