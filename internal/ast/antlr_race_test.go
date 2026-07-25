package ast

import (
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
)

// TestAntlrConcurrentParseRace parses many real .sql files concurrently through
// the native ANTLR PL/SQL driver, which shares a package-level static
// ATN / decisionToDFA / PredictionContextCache across all parser instances. The
// production pipeline runs opts.Workers goroutines over these singleton drivers,
// so this guards that antlr4-go v4.13.1's shared static state is safe under
// concurrency. Run with the race detector:
//
//	GRAPHIT_RACE_SQL_DIR=/path/to/sql go test -race -run TestAntlrConcurrentParseRace ./internal/ast/
//
// Skips when GRAPHIT_RACE_SQL_DIR is unset so it stays hermetic in CI.
func TestAntlrConcurrentParseRace(t *testing.T) {
	dir := os.Getenv("GRAPHIT_RACE_SQL_DIR")
	if dir == "" {
		t.Skip("set GRAPHIT_RACE_SQL_DIR to a directory of .sql files to run the ANTLR concurrency race check")
	}

	const (
		maxFiles = 600
		maxBytes = 32 * 1024 // skip huge files to keep the -race run bounded
	)

	var files []string
	_ = filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		if !strings.HasSuffix(strings.ToLower(p), ".sql") {
			return nil
		}
		fi, e := d.Info()
		if e != nil || fi.Size() == 0 || fi.Size() > maxBytes {
			return nil
		}
		files = append(files, p)
		if len(files) >= maxFiles {
			return filepath.SkipAll
		}
		return nil
	})
	if len(files) == 0 {
		t.Skipf("no suitable .sql files (<= %d bytes) found under %s", maxBytes, dir)
	}

	drv := nativeAntlrDrivers["antlr-plsql"]
	if drv == nil {
		t.Fatal("native antlr-plsql driver not registered")
	}

	// Warm the static ATN once so the sync.Once init is not itself the contended
	// path; the race of interest is the DFA edge cache mutated during parsing.
	if src, err := os.ReadFile(files[0]); err == nil {
		_, _ = drv.Parse(src)
	}

	workers := runtime.GOMAXPROCS(0)
	if workers < 4 {
		workers = 4
	}

	ch := make(chan string, workers)
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for p := range ch {
				src, err := os.ReadFile(p)
				if err != nil {
					continue
				}
				// Concurrent parse: exercises the shared static ATN/DFA.
				_, _ = drv.Parse(src)
			}
		}()
	}
	for _, p := range files {
		ch <- p
	}
	close(ch)
	wg.Wait()

	t.Logf("parsed %d .sql files across %d goroutines under the race detector", len(files), workers)
}
