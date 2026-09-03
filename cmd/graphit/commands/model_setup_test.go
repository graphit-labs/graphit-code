package commands

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/output"
)

func feed(r *modelProgress, file string, total int64, reads int, gap time.Duration) []string {
	var lines []string
	at := time.Now()

	collect := func(downloaded int64) {
		if line, ok := r.next(file, downloaded, total, at); ok {
			lines = append(lines, line)
		}
		at = at.Add(gap)
	}

	collect(0)
	for i := 1; i <= reads; i++ {
		collect(total * int64(i) / int64(reads))
	}
	return lines
}

// Without a cursor to move, every update is another line in a log. A 132 MB
// model arrives in some four thousand reads; one line each would bury the rest
// of setup, which is what the percentage throttle is for.
func TestNonTTYProgressSpeaksInTenthsNotPerRead(t *testing.T) {
	r := &modelProgress{tty: false}

	lines := feed(r, "model.onnx", 132<<20, 4000, 0)

	if len(lines) > 1+11 {
		t.Errorf("emitted %d lines for 4000 reads; want at most 12", len(lines))
	}
	if len(lines) < 3 {
		t.Errorf("emitted only %d lines; the download would look stalled", len(lines))
	}
	if !strings.Contains(lines[len(lines)-1], "100%") {
		t.Errorf("last line does not report completion: %q", lines[len(lines)-1])
	}
}

// The final read has to be reported even when it lands inside the tenth that
// was just announced, or the log stops at 90-something percent on every run.
func TestNonTTYProgressAlwaysReportsCompletion(t *testing.T) {
	r := &modelProgress{tty: false}

	lines := feed(r, "model.onnx", 1000, 101, 0)

	if last := lines[len(lines)-1]; !strings.Contains(last, "100%") {
		t.Errorf("last line = %q; want it to report 100%%", last)
	}
}

// A bundle is two files. The second one has to announce itself, or its progress
// is read as a continuation of the first.
func TestProgressReAnnouncesOnANewFile(t *testing.T) {
	r := &modelProgress{tty: false}

	feed(r, "model.onnx", 1000, 10, 0)
	lines := feed(r, "tokenizer.json", 1000, 10, 0)

	if len(lines) == 0 {
		t.Fatal("the second file reported nothing")
	}
	if !strings.Contains(lines[0], "tokenizer.json") {
		t.Errorf("first line for the second file = %q; want it named", lines[0])
	}
}

// On a terminal the line is rewritten in place, so the limit is the refresh
// rate rather than the number of lines.
func TestTTYProgressIsRateLimited(t *testing.T) {
	r := &modelProgress{tty: true}

	lines := feed(r, "model.onnx", 1000, 100, modelRefreshInterval/50)

	if len(lines) > 10 {
		t.Errorf("emitted %d refreshes; the rate limit is not holding", len(lines))
	}
	if len(lines) < 2 {
		t.Errorf("emitted %d refreshes; the bar would never move", len(lines))
	}
}

func TestTTYProgressRefreshesOnceTheIntervalPasses(t *testing.T) {
	r := &modelProgress{tty: true}

	lines := feed(r, "model.onnx", 1000, 10, modelRefreshInterval*2)

	if len(lines) != 11 {
		t.Errorf("emitted %d refreshes for 11 well-spaced updates; want 11", len(lines))
	}
	if !strings.Contains(lines[1], "[") {
		t.Errorf("terminal line has no bar: %q", lines[1])
	}
}

// spoke is how the setup step tells a download apart from a cache hit.
func TestProgressRecordsWhetherItSpoke(t *testing.T) {
	silent := &modelProgress{tty: false}
	if silent.spoke {
		t.Error("spoke is set before anything was reported")
	}

	if _, ok := silent.next("model.onnx", 0, 1000, time.Now()); !ok {
		t.Fatal("the opening call was throttled away")
	}
	if !silent.spoke {
		t.Error("spoke is not set after a report")
	}
}

// A throttled update still counts as having been told about a download: the
// bytes moved even if the line did not.
func TestProgressRecordsSpokeEvenWhenThrottled(t *testing.T) {
	r := &modelProgress{tty: true}

	at := time.Now()
	if _, ok := r.next("model.onnx", 0, 1000, at); !ok {
		t.Fatal("the opening call was throttled away")
	}
	if _, ok := r.next("model.onnx", 1, 1000, at); ok {
		t.Fatal("a second update in the same instant should be throttled")
	}
	if !r.spoke {
		t.Error("spoke was cleared by a throttled update")
	}
}

func isolateHome(t *testing.T, dir string) bool {
	t.Helper()

	switch runtime.GOOS {
	case "windows":
		t.Setenv("USERPROFILE", dir)
	default:
		t.Setenv("HOME", dir)
	}

	cache, err := ai.ModelCacheDir()
	if err != nil {
		return false
	}
	return strings.HasPrefix(cache, dir)
}

// The download is fatal to setup, so the error has to reach the caller rather
// than being absorbed into a warning.
func TestEnsureEmbeddingModelReturnsTheErrorItCannotRecoverFrom(t *testing.T) {
	dir := t.TempDir()
	notADir := filepath.Join(dir, "home-is-a-file")
	if err := os.WriteFile(notADir, nil, 0o644); err != nil {
		t.Fatal(err)
	}

	if !isolateHome(t, notADir) {
		t.Skip("could not redirect the home directory on this platform")
	}

	p := output.NewPrinterTo("", io.Discard)

	gotDir, downloaded, err := ensureEmbeddingModel(context.Background(), p)
	if err == nil {
		t.Fatal("no error for a model cache that cannot be created")
	}
	if downloaded {
		t.Error("reported a download that never started")
	}

	if gotDir == "" {
		t.Error("no cache directory returned; the failure message has nothing to point at")
	}
	if !strings.HasPrefix(gotDir, notADir) {
		t.Errorf("returned %q; want a path under the redirected home %q", gotDir, notADir)
	}
}

// A cache that is already valid must not be reported as a download, or setup
// tells a first-time user it fetched 132 MB it did not fetch.
func TestEnsureEmbeddingModelReportsACacheHitAsSuchAndDoesNotFail(t *testing.T) {
	home := t.TempDir()
	if !isolateHome(t, home) {
		t.Skip("could not redirect the home directory on this platform")
	}

	cache, err := ai.ModelCacheDir()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(cache, 0o755); err != nil {
		t.Fatal(err)
	}
	for name, size := range map[string]int64{
		"model.onnx":     150_000_000,
		"tokenizer.json": 600_000,
	} {
		f, createErr := os.Create(filepath.Join(cache, name))
		if createErr != nil {
			t.Fatal(createErr)
		}
		if truncErr := f.Truncate(size); truncErr != nil {
			t.Fatal(truncErr)
		}
		if closeErr := f.Close(); closeErr != nil {
			t.Fatal(closeErr)
		}
	}

	p := output.NewPrinterTo("", io.Discard)

	gotDir, downloaded, err := ensureEmbeddingModel(context.Background(), p)
	if err != nil {
		t.Fatalf("a valid cache should not be an error: %v", err)
	}
	if downloaded {
		t.Error("reported a download for a cache hit")
	}
	if gotDir != cache {
		t.Errorf("returned %q; want the cache %q", gotDir, cache)
	}
}
