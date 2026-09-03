package ai

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
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

func artifactServer(t *testing.T, content map[string][]byte) string {
	t.Helper()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, ok := content[r.URL.Path]
		if !ok {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		_, _ = w.Write(body)
	}))
	t.Cleanup(srv.Close)
	return srv.URL
}

func seedModelCache(t *testing.T, home string) {
	t.Helper()
	dir := filepath.Join(home, brand.DotDir(), modelCacheSubdir)
	sparseFile(t, filepath.Join(dir, modelFileName), modelONNXMinSize+1)
	sparseFile(t, filepath.Join(dir, tokenizerFileName), tokenizerJSONMinSize+1)
}
