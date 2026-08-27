package ai

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"testing"
)

// serveBytes hands out n zero bytes, declaring the length only when told to.
// A server that declares none is not an error case: download has to keep
// working, and the progress hook has to say so rather than invent a total.
func serveBytes(t *testing.T, n int64, declareLength bool) string {
	t.Helper()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if declareLength {
			w.Header().Set("Content-Length", strconv.FormatInt(n, 10))
		}
		w.WriteHeader(http.StatusOK)
		_, _ = writeZeros(w, n)
	}))
	t.Cleanup(srv.Close)

	return srv.URL + "/" + modelFileName
}

type progressCall struct {
	file       string
	downloaded int64
	total      int64
}

func TestDownloadReportsProgressFromZeroToTheFullSize(t *testing.T) {
	const size = 3 << 20

	dest := filepath.Join(t.TempDir(), modelFileName)
	url := serveBytes(t, size, true)

	var calls []progressCall
	m := &ModelManager{
		OnProgress: func(file string, downloaded, total int64) {
			calls = append(calls, progressCall{file, downloaded, total})
		},
	}

	if err := m.download(context.Background(), url, dest); err != nil {
		t.Fatalf("download: %v", err)
	}

	if len(calls) < 2 {
		t.Fatalf("got %d progress calls; want an opening one plus at least one read", len(calls))
	}

	// The opening call exists so a caller can announce the download before
	// there is anything to report.
	if first := calls[0]; first.downloaded != 0 || first.total != size {
		t.Errorf("first call = %+v; want downloaded 0 and total %d", first, int64(size))
	}

	if last := calls[len(calls)-1]; last.downloaded != size {
		t.Errorf("last call reported %d of %d bytes; want the full size", last.downloaded, int64(size))
	}

	var prev int64 = -1
	for i, c := range calls {
		if c.downloaded < prev {
			t.Fatalf("call %d went backwards: %d after %d", i, c.downloaded, prev)
		}
		prev = c.downloaded

		// The bytes land in a .tmp file, but the caller is showing this to a
		// person who asked for a model.
		if c.file != modelFileName {
			t.Errorf("call %d reported file %q; want %q", i, c.file, modelFileName)
		}
	}
}

func TestDownloadReportsNoTotalWhenTheServerDeclaresNone(t *testing.T) {
	dest := filepath.Join(t.TempDir(), modelFileName)
	url := serveBytes(t, 64<<10, false)

	var totals []int64
	m := &ModelManager{
		OnProgress: func(_ string, _, total int64) {
			totals = append(totals, total)
		},
	}

	if err := m.download(context.Background(), url, dest); err != nil {
		t.Fatalf("download: %v", err)
	}

	if len(totals) == 0 {
		t.Fatal("no progress reported")
	}
	for i, total := range totals {
		if total > 0 {
			t.Fatalf("call %d claimed a total of %d; the server declared none", i, total)
		}
	}
}

// The hook is optional, and every non-interactive caller leaves it nil.
func TestDownloadWithoutAProgressHookStillWritesTheFile(t *testing.T) {
	const size = 128 << 10

	dest := filepath.Join(t.TempDir(), modelFileName)
	url := serveBytes(t, size, true)

	m := &ModelManager{}
	if err := m.download(context.Background(), url, dest); err != nil {
		t.Fatalf("download: %v", err)
	}

	info, err := os.Stat(dest)
	if err != nil {
		t.Fatalf("stat downloaded file: %v", err)
	}
	if info.Size() != size {
		t.Errorf("wrote %d bytes; want %d", info.Size(), int64(size))
	}
}

// EnsureModel is the whole reason the hook exists, and it downloads two files.
// Both have to be named, or a progress line cannot say which one is moving.
func TestEnsureModelReportsBothFilesByName(t *testing.T) {
	cacheDir := t.TempDir()
	m := modelServer(t, cacheDir)

	seen := map[string]bool{}
	m.OnProgress = func(file string, _, _ int64) { seen[file] = true }

	if _, _, err := m.EnsureModel(context.Background()); err != nil {
		t.Fatalf("EnsureModel: %v", err)
	}

	for _, want := range []string{modelFileName, tokenizerFileName} {
		if !seen[want] {
			t.Errorf("no progress reported for %s", want)
		}
	}
}

// A cache hit reports nothing, which is how the setup step tells "downloaded"
// apart from "was already there" without a second question.
func TestEnsureModelReportsNothingWhenTheCacheIsAlreadyValid(t *testing.T) {
	cacheDir := t.TempDir()
	sparseFile(t, filepath.Join(cacheDir, modelFileName), modelONNXMinSize+1)
	sparseFile(t, filepath.Join(cacheDir, tokenizerFileName), tokenizerJSONMinSize+1)

	called := false
	m := &ModelManager{
		cacheDir:   cacheDir,
		OnProgress: func(string, int64, int64) { called = true },
	}

	if _, _, err := m.EnsureModel(context.Background()); err != nil {
		t.Fatalf("EnsureModel: %v", err)
	}
	if called {
		t.Error("progress reported for a cache hit; nothing was downloaded")
	}
}
