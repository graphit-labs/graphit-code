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

// The model is 132 MB and lives on huggingface.co. Seven tests in this package
// used to reach it for real — three by constructing a ModelManager whose URLs
// were compile-time constants, four by going through NewEmbeddingClientFromConfig
// with $HOME pointed at an empty temp dir. Together they were ~81 of the
// package's ~103 seconds, and they made `make test` fail whenever huggingface.co
// was slow or unreachable. Worse, every one of them asserted
// `if err == nil { t.Log(...) }` — the outcome was whatever the network gave, so
// nothing was actually being pinned.
//
// The two helpers here remove the network from both routes.

// sparseFile writes a file of exactly size bytes without allocating or storing
// them. isValid only ever calls Stat, so the size is the whole point; a
// make([]byte, 100_000_000) per test is 100 MB of heap and 100 MB of writes to
// answer a question about st_size.
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

// modelServer returns a ModelManager whose downloads land on a local server that
// serves files of exactly the minimum accepted size, and the URLs to reach it.
// The bytes are zeros: nothing downstream of EnsureModel parses them, it only
// stats them.
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
		// Content-Length must match what is written: download() treats a short
		// body as an incomplete download and deletes it.
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

// writeZeros writes n zero bytes in modest chunks rather than materialising a
// 100 MB buffer to hand to Write.
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

// seedModelCache fills the cache directory that NewModelManager derives from
// $HOME, so EnsureModel finds valid files and returns before it ever considers
// downloading. This is the route for tests that go through
// NewEmbeddingClientFromConfig / NewLocalEmbeddingClient, which build their own
// ModelManager and have nowhere to inject a URL.
//
// The files are zeros, so ONNX still refuses to load the model — which is the
// error path those tests already accept. What changes is that they reach it in
// milliseconds instead of after a 132 MB download.
func seedModelCache(t *testing.T, home string) {
	t.Helper()
	dir := filepath.Join(home, brand.DotDir(), modelCacheSubdir)
	sparseFile(t, filepath.Join(dir, modelFileName), modelONNXMinSize+1)
	sparseFile(t, filepath.Join(dir, tokenizerFileName), tokenizerJSONMinSize+1)
}
