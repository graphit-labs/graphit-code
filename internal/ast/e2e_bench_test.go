package ast

import (
	"context"
	"fmt"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// TestFindPathologicalSQL parses each .sql file in GRAPHIT_PATHO_SQL_DIR one at a
// time (single-threaded) and reports the worst offenders by wall time and by RSS
// high-water-mark growth. ANTLR's full-context LL fallback can explode on
// ambiguous input regardless of file size; this pinpoints which file does it.
//
//	GRAPHIT_PATHO_SQL_DIR=/path go test ./internal/ast/ -run TestFindPathologicalSQL -v -count=1
func TestFindPathologicalSQL(t *testing.T) {
	dir := os.Getenv("GRAPHIT_PATHO_SQL_DIR")
	if dir == "" {
		t.Skip("set GRAPHIT_PATHO_SQL_DIR to scan a directory for pathological SQL files")
	}
	var files []string
	_ = filepath.WalkDir(dir, func(p string, d fs.DirEntry, e error) error {
		if e == nil && !d.IsDir() && strings.HasSuffix(strings.ToLower(p), ".sql") {
			files = append(files, p)
		}
		return nil
	})
	sort.Strings(files)
	if skip := os.Getenv("GRAPHIT_PATHO_SKIP"); skip != "" {
		var n int
		_, _ = fmt.Sscanf(skip, "%d", &n)
		if n < len(files) {
			files = files[n:]
		}
	}
	t.Logf("scanning %d files (single-threaded)", len(files))

	drv := nativeAntlrDrivers["antlr-plsql"]
	if drv == nil {
		t.Fatal("antlr-plsql driver not registered")
	}
	type rec struct {
		path string
		size int64
		ms   int64
		rss  int64
	}
	var recs []rec
	for _, p := range files {
		src, err := os.ReadFile(p)
		if err != nil {
			continue
		}
		// Print BEFORE parsing, unbuffered: t.Logf is buffered until the test
		// ends, so if a file OOMs the process the culprit's name would be lost.
		fmt.Fprintf(os.Stderr, "PARSING %s (%dB) rss=%dMB\n", filepath.Base(p), len(src), currentRSSMB())
		before := peakRSSMB()
		t0 := time.Now()
		_, _ = drv.Parse(src)
		el := time.Since(t0).Milliseconds()
		runtime.GC()
		grew := peakRSSMB() - before
		recs = append(recs, rec{p, int64(len(src)), el, grew})
		if el > 500 || grew > 200 {
			fmt.Fprintf(os.Stderr, "  !! HEAVY %s size=%dB %dms peakRSS+%dMB\n", filepath.Base(p), len(src), el, grew)
		}
	}
	sort.Slice(recs, func(i, j int) bool { return recs[i].ms > recs[j].ms })
	t.Logf("=== top 10 by parse time ===")
	for i := 0; i < 10 && i < len(recs); i++ {
		r := recs[i]
		t.Logf("  %6dms  peak+%4dMB  %7dB  %s", r.ms, r.rss, r.size, filepath.Base(r.path))
	}
	sort.Slice(recs, func(i, j int) bool { return recs[i].rss > recs[j].rss })
	t.Logf("=== top 10 by peak RSS growth ===")
	for i := 0; i < 10 && i < len(recs); i++ {
		r := recs[i]
		t.Logf("  peak+%5dMB  %6dms  %7dB  %s", r.rss, r.ms, r.size, filepath.Base(r.path))
	}
}

// e2eGrammarOverrides parses GRAPHIT_E2E_GRAMMAR (".sql=antlr-plsql", comma
// separated) into pipeline grammar overrides. Empty when unset.
func e2eGrammarOverrides() map[string]string {
	v := os.Getenv("GRAPHIT_E2E_GRAMMAR")
	if v == "" {
		return nil
	}
	out := map[string]string{}
	for _, pair := range strings.Split(v, ",") {
		if ext, g, ok := strings.Cut(strings.TrimSpace(pair), "="); ok {
			out[ext] = g
		}
	}
	return out
}

// rssSampler polls RSS continuously so short-lived spikes (e.g. an ANTLR
// full-context LL parse of a pathological file, which coarse per-N-files
// sampling would miss entirely) cannot hide between samples.
type rssSampler struct {
	stop chan struct{}
	done chan struct{}
	max  atomic.Int64
}

func startRSSSampler(every time.Duration) *rssSampler {
	s := &rssSampler{stop: make(chan struct{}), done: make(chan struct{})}
	go func() {
		defer close(s.done)
		t := time.NewTicker(every)
		defer t.Stop()
		for {
			select {
			case <-s.stop:
				return
			case <-t.C:
				if v := currentRSSMB(); v > s.max.Load() {
					s.max.Store(v)
				}
			}
		}
	}()
	return s
}

func (s *rssSampler) Max() int64 { return s.max.Load() }

func (s *rssSampler) Stop() int64 {
	close(s.stop)
	<-s.done
	return s.max.Load()
}

// currentRSSMB reads the process's current RSS (Linux) in MiB.
func currentRSSMB() int64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "VmRSS:") {
			var kb int64
			_, _ = fmt.Sscanf(strings.TrimSpace(strings.TrimPrefix(line, "VmRSS:")), "%d", &kb)
			return kb / 1024
		}
	}
	return 0
}

// peakRSSMB reads the process high-water-mark RSS (Linux) in MiB.
func peakRSSMB() int64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "VmHWM:") {
			var kb int64
			_, _ = fmt.Sscanf(strings.TrimSpace(strings.TrimPrefix(line, "VmHWM:")), "%d", &kb)
			return kb / 1024
		}
	}
	return 0
}

// TestE2EIndex runs the full pipeline (discover -> hash -> parse -> cache ->
// LadybugDB + search index) over a real corpus, then an incremental re-index of
// a single changed file, reporting per-phase timings and peak RSS. Gated by
// GRAPHIT_E2E_SQL_DIR; the corpus is copied to a temp workdir so the source is
// never modified. Run:
//
//	GRAPHIT_E2E_SQL_DIR=/path/to/sql go test ./internal/ast/ -run TestE2EIndex -v -count=1 -timeout 30m
func TestE2EIndex(t *testing.T) {
	src := os.Getenv("GRAPHIT_E2E_SQL_DIR")
	if src == "" {
		t.Skip("set GRAPHIT_E2E_SQL_DIR to a corpus directory to run the end-to-end index benchmark")
	}

	tmp := t.TempDir()
	work := filepath.Join(tmp, "corpus")

	// Copy at most GRAPHIT_E2E_MAX_FILES files (0 = all) so memory growth can be
	// measured on bounded subsets.
	maxFiles := 0
	if v := os.Getenv("GRAPHIT_E2E_MAX_FILES"); v != "" {
		_, _ = fmt.Sscanf(v, "%d", &maxFiles)
	}
	if err := os.MkdirAll(work, 0o755); err != nil {
		t.Fatal(err)
	}
	copied := 0
	err := filepath.WalkDir(src, func(p string, d fs.DirEntry, e error) error {
		if e != nil || d.IsDir() {
			return nil
		}
		if maxFiles > 0 && copied >= maxFiles {
			return filepath.SkipAll
		}
		b, rerr := os.ReadFile(p)
		if rerr != nil {
			return nil
		}
		rel, _ := filepath.Rel(src, p)
		dst := filepath.Join(work, rel)
		if mkErr := os.MkdirAll(filepath.Dir(dst), 0o755); mkErr != nil {
			return nil
		}
		if wErr := os.WriteFile(dst, b, 0o644); wErr == nil {
			copied++
		}
		return nil
	})
	if err != nil {
		t.Fatalf("copy corpus: %v", err)
	}
	t.Logf("corpus: %d files copied from %s (workers=%d)", copied, src, SafeWorkers(0))

	dbPath := filepath.Join(tmp, "ladybugdb")
	cacheDir := filepath.Join(tmp, "cache")
	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	// Progressive RSS sampling: a peak that grows with the NUMBER of files
	// processed (rather than with worker count) indicates cumulative retention
	// (e.g. the ANTLR static DFA/ATN caches, which never evict) rather than
	// per-parse tree size.
	// Surface the write-path timing breakdown (copy/open/delete/insert/enrich)
	// that IncrementalRebuild logs at Info level.
	opts := PipelineOptions{
		CacheDir:    cacheDir,
		IndexSource: true,
		Logger:      slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelInfo})),
		// GRAPHIT_E2E_GRAMMAR pins one grammar per extension (e.g.
		// ".sql=antlr-plsql") to measure the cost of trying every candidate
		// grammar, each of which warms its own ANTLR static caches.
		GrammarOverrides: e2eGrammarOverrides(),
		OnProgress: func(phase string, cur, total, errs int) {
			if cur > 0 && cur%250 == 0 {
				t.Logf("  ...%s %d/%d errs=%d rss=%dMB", phase, cur, total, errs, currentRSSMB())
			}
		},
	}

	// ---- FULL ----
	t0 := time.Now()
	res, err := RunPipeline(ctx, db, work, opts)
	if err != nil {
		t.Fatalf("full pipeline: %v", err)
	}
	full := time.Since(t0)
	t.Logf("FULL  files=%d parsed=%d empty=%d errors=%d | discover=%v hash=%v parse=%v write=%v total=%v | peakRSS=%dMB",
		res.TotalFiles, res.ParsedFiles, res.EmptyCount, res.ErrorCount,
		res.DiscoverTime.Round(time.Millisecond), res.HashTime.Round(time.Millisecond),
		res.ParseTime.Round(time.Millisecond), res.WriteTime.Round(time.Millisecond),
		full.Round(time.Millisecond), peakRSSMB())

	// ---- INCREMENTAL (single changed file) ----
	var one string
	_ = filepath.WalkDir(work, func(p string, d fs.DirEntry, e error) error {
		if e == nil && one == "" && !d.IsDir() && strings.HasSuffix(strings.ToLower(p), ".sql") {
			one = p
			return filepath.SkipAll
		}
		return nil
	})
	if one == "" {
		t.Logf("no .sql file found for incremental step; skipping")
		return
	}
	f, err := os.OpenFile(one, os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		t.Fatalf("open for touch: %v", err)
	}
	_, _ = f.WriteString("\n-- e2e benchmark touch\n")
	_ = f.Close()

	t1 := time.Now()
	res2, err := RunPipeline(ctx, db, work, opts)
	if err != nil {
		t.Fatalf("incremental pipeline: %v", err)
	}
	incr := time.Since(t1)
	// Alternate scoped and discovery incrementals so a per-run cost that grows
	// with position (evolving DB state) can be told apart from a cost caused by
	// the mode itself.
	for round := 1; round <= 6; round++ {
		for _, mode := range []string{"SCOPED"} {
			fx, oErr := os.OpenFile(one, os.O_APPEND|os.O_WRONLY, 0o644)
			if oErr != nil {
				break
			}
			_, _ = fx.WriteString("\n-- e2e alternating touch\n")
			_ = fx.Close()

			tm := time.Now()
			var r *PipelineResult
			var rErr error
			if mode == "SCOPED" {
				r, rErr = RunPipelineForPaths(ctx, db, work, []string{one}, nil, opts)
			} else {
				r, rErr = RunPipeline(ctx, db, work, opts)
			}
			if rErr != nil {
				t.Fatalf("%s pipeline: %v", mode, rErr)
			}
			// Correctness: the changed file must be present in the swapped-in DB.
			relOne, _ := filepath.Rel(work, one)
			vres, vErr := db.Query(ctx, "MATCH (f:File {path: $p}) RETURN count(f) AS c", map[string]any{"p": relOne})
			var nodes int64
			if vErr == nil && len(vres.Records) > 0 {
				switch v := vres.Records[0]["c"].(type) {
				case int64:
					nodes = v
				case int:
					nodes = int64(v)
				}
			}
			if nodes != 1 {
				t.Errorf("round=%d %s: changed file %q not in graph after swap (count=%d)", round, mode, relOne, nodes)
			}

			t.Logf("AB round=%d %-9s parsed=%d | discover=%v hash=%v parse=%v write=%v total=%v",
				round, mode, r.ParsedFiles,
				r.DiscoverTime.Round(time.Millisecond), r.HashTime.Round(time.Millisecond),
				r.ParseTime.Round(time.Millisecond), r.WriteTime.Round(time.Millisecond),
				time.Since(tm).Round(time.Millisecond))
		}
	}

	t.Logf("INCR  files=%d parsed=%d errors=%d | discover=%v hash=%v parse=%v write=%v total=%v | peakRSS=%dMB",
		res2.TotalFiles, res2.ParsedFiles, res2.ErrorCount,
		res2.DiscoverTime.Round(time.Millisecond), res2.HashTime.Round(time.Millisecond),
		res2.ParseTime.Round(time.Millisecond), res2.WriteTime.Round(time.Millisecond),
		incr.Round(time.Millisecond), peakRSSMB())
}
