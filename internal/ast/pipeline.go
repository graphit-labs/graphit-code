package ast

import (
	"context"
	"crypto/sha256"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// antlrCacheCheckInterval is how many files are parsed between memory checks.
// Each check is a barrier (all workers drained), which is also the only point
// where the shared ANTLR caches may safely be reset.
var antlrCacheCheckInterval = func() int {
	if n := envUint("GRAPHIT_ANTLR_RESET_FILES"); n > 0 {
		return int(n)
	}
	return 250
}()

// antlrCacheHeapLimit is the Go-heap ceiling above which the ANTLR caches are
// reset at the next barrier. Resetting is PRESSURE-DRIVEN rather than on a fixed
// file count because each reset discards a warm DFA and measurably slows parsing
// (measured: resetting every 500 files cost ~78% more parse time), while never
// resetting let the heap reach 23 GB on a 35k-file Oracle corpus. Checking the
// heap means small repositories never pay a reset and large ones pay only as many
// as needed. The budget scales with the machine — see AntlrHeapBudget.
var antlrCacheHeapLimit = AntlrHeapBudget()

// antlrCachePressure reports whether the Go heap has grown past the limit. It
// uses runtime.MemStats (not /proc) so the check works on Linux, macOS and
// Windows alike; the ANTLR DFA / prediction-context caches are ordinary Go heap
// allocations, so HeapInuse tracks them directly.
func antlrCachePressure() (uint64, bool) {
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	return ms.HeapInuse, ms.HeapInuse > antlrCacheHeapLimit
}

type PipelineOptions struct {
	Workers          int
	IsDepend         bool
	IndexSource      bool
	SkipExternal     bool
	CacheDir         string
	ExcludeExts      map[string]bool
	GrammarOverrides map[string]string
	Cluster          string
	ForceRebuild     bool
	Logger           *slog.Logger
	OnProgress       func(phase string, current, total, errors int)

	// ChangedPaths / DeletedPaths let a caller that already knows exactly what
	// changed (a filesystem watcher) skip discovery entirely. Without them every
	// run walks the whole tree and stats every file just to learn that one file
	// changed — measured at 344 ms of a 1.07 s incremental on a 35k-file repo.
	// ChangedPaths are filesystem paths; DeletedPaths are repo-relative, matching
	// the parse cache's keys.
	ChangedPaths []string
	DeletedPaths []string
}

type PipelineResult struct {
	TotalFiles   int
	ParsedFiles  int
	DiscoverTime time.Duration
	HashTime     time.Duration
	ParseTime    time.Duration
	WriteTime    time.Duration
	TotalTime    time.Duration

	ErrorCount      int
	TimeoutCount    int
	EmptyCount      int
	WriteErrorCount int
	EmptyFiles      []string
	ErrorFiles      []string
	WriteErrorFiles []string

	EngineStats map[string]int
}

func RunPipeline(ctx context.Context, db GraphDB, rootPath string, opts PipelineOptions) (*PipelineResult, error) {
	if opts.Workers <= 0 {
		opts.Workers = SafeWorkers(0)
	} else {
		opts.Workers = SafeWorkers(opts.Workers)
	}
	t0 := time.Now()
	abs, _ := filepath.Abs(rootPath)

	writer := NewGraphWriter(db, abs, opts.IndexSource)
	writer.cluster = opts.Cluster

	parser := NewCompositeParser(abs, opts.GrammarOverrides)
	return runFileWorkerPool(ctx, db, writer, abs, parser, t0, opts)
}

// RunPipelineForPaths indexes only the named changed and deleted paths, skipping
// discovery (the full tree walk plus a stat of every file). Use it when a
// filesystem watcher already knows what changed: on a 35k-file repository that
// discovery costs ~344 ms of a ~1.07 s incremental run.
//
// changed are filesystem paths; deleted are repo-relative, matching parse-cache keys.
func RunPipelineForPaths(ctx context.Context, db GraphDB, rootPath string, changed, deleted []string, opts PipelineOptions) (*PipelineResult, error) {
	opts.ChangedPaths = changed
	opts.DeletedPaths = deleted
	return RunPipeline(ctx, db, rootPath, opts)
}

func runFileWorkerPool(ctx context.Context, db GraphDB, writer *GraphWriter, abs string, parser LanguageParser, t0 time.Time, opts PipelineOptions) (*PipelineResult, error) {
	// scoped: the caller named the changed/deleted paths, so the tree walk and
	// the stat-every-file pass are unnecessary.
	scoped := len(opts.ChangedPaths) > 0 || len(opts.DeletedPaths) > 0

	tDiscover := time.Now()
	var files []string
	if !scoped {
		var err error
		files, err = collectFiles(abs)
		if err != nil {
			return nil, fmt.Errorf("discover files: %w", err)
		}

		if len(opts.ExcludeExts) > 0 {
			var filtered []string
			for _, f := range files {
				ext := strings.ToLower(filepath.Ext(f))
				if !opts.ExcludeExts[ext] {
					filtered = append(filtered, f)
				}
			}
			files = filtered
		}
	}
	discoverTime := time.Since(tDiscover)

	logger := slogutil.Resolve(opts.Logger)

	var jsonCache *ShardCache
	if opts.CacheDir != "" {
		var err error
		jsonCache, err = NewShardCache(opts.CacheDir)
		if err != nil {
			logger.Warn("json cache fallback", "error", err)
		}
	}

	tHash := time.Now()
	var changedFiles []string
	var deletedFiles []string
	fileHashes := make(map[string]string, len(files))
	if scoped {
		// Only the named files are considered. Hash them (in parallel) so the
		// parse cache stores the right content hash; everything else is untouched.
		deletedFiles = append(deletedFiles, opts.DeletedPaths...)
		if jsonCache != nil {
			for _, rel := range opts.DeletedPaths {
				jsonCache.Remove(rel)
			}
		}
		if len(opts.ChangedPaths) > 0 {
			type hres struct {
				path, hash string
			}
			parallelForEach(opts.ChangedPaths, SafeWorkers(0),
				func(p string) hres { return hres{p, fileContentHash(p)} },
				func(r hres) {
					if r.hash == "" {
						return // unreadable/vanished — skip rather than index garbage
					}
					fileHashes[r.path] = r.hash
					changedFiles = append(changedFiles, r.path)
				},
			)
		}
	} else if jsonCache != nil && !opts.ForceRebuild {
		// Phase A: stat all files in parallel — much cheaper than SHA-256.
		// Files whose mtime matches the cached mtime are skipped entirely.
		type statResult struct {
			path  string
			rel   string
			mtime int64
			skip  bool // mtime unchanged → treat as not changed
		}
		// Collect stat results and partition into needHash vs confirmed-unchanged.
		type fileInfo struct {
			path  string
			rel   string
			mtime int64
		}
		var needHash []fileInfo
		var mtimeUnchanged []fileInfo // rel != "" && mtime matched
		liveFiles := make(map[string]bool, len(files))

		// Fixed worker pool over a bounded channel — O(workers) memory even for
		// repos with tens of thousands of files (vs a goroutine + channel slot
		// per file).
		parallelForEach(files, SafeWorkers(0),
			func(path string) statResult {
				fAbs, _ := filepath.Abs(path)
				rel := writer.rel(fAbs)
				info, err := os.Stat(path)
				if err != nil {
					return statResult{path: path, rel: rel}
				}
				mtime := info.ModTime().UnixNano()
				skip := rel != "" && !jsonCache.NeedsHash(rel, mtime)
				return statResult{path: path, rel: rel, mtime: mtime, skip: skip}
			},
			func(sr statResult) {
				if sr.rel != "" {
					liveFiles[sr.rel] = true
				}
				if sr.skip {
					mtimeUnchanged = append(mtimeUnchanged, fileInfo{sr.path, sr.rel, sr.mtime})
				} else {
					needHash = append(needHash, fileInfo{sr.path, sr.rel, sr.mtime})
				}
			},
		)

		// Phase B: SHA-256 only the files that need it (mtime changed or no mtime cached).
		if len(needHash) > 0 {
			type hashResult struct {
				path  string
				hash  string
				rel   string
				mtime int64
			}
			parallelForEach(needHash, SafeWorkers(0),
				func(fi fileInfo) hashResult {
					return hashResult{path: fi.path, hash: fileContentHash(fi.path), rel: fi.rel, mtime: fi.mtime}
				},
				func(hr hashResult) {
					fileHashes[hr.path] = hr.hash
					if hr.rel == "" {
						return
					}
					if jsonCache.HasChanged(hr.rel, hr.hash) {
						changedFiles = append(changedFiles, hr.path)
					} else {
						// Hash confirmed unchanged — update mtime so next sync is faster.
						jsonCache.StoreMtime(hr.rel, hr.mtime)
					}
				},
			)
		}

		// Detect deleted files (in cache but not on disk).
		for _, cached := range jsonCache.AllPaths() {
			if !liveFiles[cached] {
				deletedFiles = append(deletedFiles, cached)
				jsonCache.Remove(cached)
			}
		}

		// mtimeUnchanged files: add their paths to fileHashes using cached hash.
		for _, fi := range mtimeUnchanged {
			fileHashes[fi.path] = jsonCache.GetHash(fi.rel)
		}
	} else {
		// Full rebuild or no cache: skip hashing, treat all files as changed.
		changedFiles = files
	}
	hashTime := time.Since(tHash)

	// In scoped mode the tree was never walked, so the corpus size comes from the
	// parse cache rather than from a file listing.
	totalFiles := len(files)
	if scoped && jsonCache != nil {
		totalFiles = jsonCache.Count()
	}

	if len(changedFiles) == 0 && len(deletedFiles) == 0 && jsonCache != nil && jsonCache.Count() > 0 && !opts.ForceRebuild {

		_ = jsonCache.Save()
		_ = jsonCache.Close()
		totalTime := time.Since(t0)
		return &PipelineResult{
			TotalFiles:   totalFiles,
			ParsedFiles:  0,
			DiscoverTime: discoverTime,
			HashTime:     hashTime,
			TotalTime:    totalTime,
			EngineStats:  make(map[string]int),
		}, nil
	}

	t1 := time.Now()
	parseOpts := ParseOptions{IndexSource: opts.IndexSource}

	dryRun := os.Getenv("AST_DRY_RUN") == "1"

	type result struct {
		path string
		pf   *ParsedFile
		err  error
	}
	results := make(chan result, 64)

	// Parse in chunks so that between chunks every worker has drained — a
	// barrier where no parse is in flight. ANTLR's package-level DFA /
	// prediction-context caches grow with every newly seen input pattern and
	// are never evicted (~2 MB per PL/SQL file measured), so without a
	// periodic reset a large repository exhausts RAM. Resetting only at the
	// barrier keeps the parse hot path lock-free.
	go func() {
		defer close(results)

		for start := 0; start < len(changedFiles); start += antlrCacheCheckInterval {
			end := start + antlrCacheCheckInterval
			if end > len(changedFiles) {
				end = len(changedFiles)
			}
			chunk := changedFiles[start:end]

			paths := make(chan string)
			go func(files []string) {
				for _, f := range files {
					paths <- f
				}
				close(paths)
			}(chunk)

			var wg sync.WaitGroup
			for i := 0; i < opts.Workers; i++ {
				wg.Add(1)
				go func() {
					defer wg.Done()
					wp := NewCompositeParser(abs, opts.GrammarOverrides)
					for path := range paths {
						pf, err := wp.Parse(path, opts.IsDepend, parseOpts)
						results <- result{path, pf, err}
					}
				}()
			}
			wg.Wait() // barrier: no parse in flight

			// Only reset when the heap is actually under pressure — a warm
			// DFA is worth keeping. This also runs after the final chunk:
			// the caches are package-level and stay LIVE (not garbage), so a
			// long-lived daemon would otherwise hold the whole budget until
			// the process exits.
			if heap, over := antlrCachePressure(); over {
				ResetAntlrCaches()
				logger.Debug("antlr caches reset", "heap_mb", heap>>20, "parsed", end)
			}
		}
	}()

	var (
		parsedFilesCount int
		parseErrors      int
		emptyCount       int
		timeoutCount     int
		writeErrors      int
		emptyFiles       []string
		errorFiles       []string
		writeErrorFiles  []string
		writeDuration    time.Duration
	)
	engineStats := make(map[string]int)

	// Determine engine label per file based on extension

	for r := range results {
		if r.err != nil {
			parseErrors++
			errEntry := r.path + ": " + r.err.Error()
			if r.path == "" {
				errEntry = r.err.Error()
			}
			errorFiles = append(errorFiles, errEntry)
			continue
		}

		if r.pf.EntityCount() == 0 {
			emptyCount++
			emptyFiles = append(emptyFiles, r.pf.Path)
		}

		label := r.pf.Parser
		if label == "" {
			label = "unknown"
		}
		langKey := label + ":" + r.pf.Language
		if r.pf.Language == "" {
			langKey = label + ":unknown"
		}
		engineStats[langKey]++

		if jsonCache != nil && !dryRun {
			entry := ConvertToCache(r.pf, abs, opts.IndexSource, opts.Cluster)
			if entry != nil {
				fAbs, _ := filepath.Abs(r.pf.Path)
				relPath := writer.rel(fAbs)
				hash := fileHashes[r.pf.Path]
				if hash == "" {
					hash = fileContentHash(r.pf.Path)
				}
				if storeErr := jsonCache.Store(relPath, hash, entry); storeErr != nil {
					writeErrors++
					writeErrorFiles = append(writeErrorFiles, fmt.Sprintf("%s: cache store: %v", relPath, storeErr))
				} else {
					// Record current mtime so the next sync can skip SHA-256 for this file.
					if info, statErr := os.Stat(r.pf.Path); statErr == nil {
						jsonCache.StoreMtime(relPath, info.ModTime().UnixNano())
					}
				}
			}
		}
		r.pf = nil

		parsedFilesCount++

		if jsonCache != nil && parsedFilesCount%100 == 0 {
			_ = jsonCache.FlushDirty()
		}

		if opts.OnProgress != nil {
			opts.OnProgress("parsing", parsedFilesCount, len(changedFiles), parseErrors+writeErrors)
		}
	}

	parseTime := time.Since(t1)

	if jsonCache != nil && !dryRun {
		_ = jsonCache.Save()
	}

	if !dryRun && jsonCache != nil && jsonCache.Count() > 0 {
		tw0 := time.Now()

		var embCache *ShardEmbCache
		if opts.CacheDir != "" {
			if ec, err := NewShardEmbCache(opts.CacheDir, jsonCache); err == nil {
				embCache = ec
			}
		}

		lb, isLB := db.(*LadybugBackend)
		productionDBExists := false
		if isLB {
			if _, statErr := os.Stat(lb.cfg.DBPath); statErr == nil {
				productionDBExists = true
			}
		}

		useIncremental := isLB && productionDBExists && !opts.ForceRebuild &&
			(len(changedFiles)+len(deletedFiles)) < jsonCache.Count()

		var searchIdx *SearchIndex
		if isLB {
			idxPath := lb.cfg.DBPath + searchIndexSuffix
			if si, err := OpenSearchIndex(idxPath); err == nil {
				searchIdx = si
				defer func() { _ = searchIdx.Close() }()
			}
		}

		var err error
		if useIncremental {

			changedRels := make([]string, 0, len(changedFiles))
			for _, f := range changedFiles {
				fAbs, _ := filepath.Abs(f)
				rel := writer.rel(fAbs)
				if rel != "" {
					changedRels = append(changedRels, rel)
				}
			}

			logger.Info("strategy selected", "type", "incremental",
				"changed", len(changedRels), "deleted", len(deletedFiles), "total", jsonCache.Count())
			err = IncrementalRebuild(ctx, lb, jsonCache, embCache, changedRels, deletedFiles, opts.Cluster, abs, searchIdx, opts.Logger)
		} else {
			logger.Info("strategy selected", "type", "full-rebuild", "files", jsonCache.Count())
			err = RebuildFromJSON(ctx, db, jsonCache, embCache, opts.Cluster, abs, opts.Logger)
			if err == nil && searchIdx != nil {
				embLookup := BuildEmbLookup(jsonCache, embCache)
				if err := searchIdx.RebuildFromCache(jsonCache, embLookup); err != nil {
					logger.Error("search index rebuild failed; search results are stale", "error", err)
				}
			}
		}
		if err != nil {
			writeErrors++
			writeErrorFiles = append(writeErrorFiles, fmt.Sprintf("rebuild: %v", err))
		}
		writeDuration = time.Since(tw0)
	}

	if jsonCache != nil {
		_ = jsonCache.Close()
	}

	totalTime := time.Since(t0)

	return &PipelineResult{
		TotalFiles:      totalFiles,
		ParsedFiles:     parsedFilesCount,
		DiscoverTime:    discoverTime,
		HashTime:        hashTime,
		ParseTime:       parseTime,
		WriteTime:       writeDuration,
		TotalTime:       totalTime,
		ErrorCount:      parseErrors,
		TimeoutCount:    timeoutCount,
		EmptyCount:      emptyCount,
		WriteErrorCount: writeErrors,
		EmptyFiles:      emptyFiles,
		ErrorFiles:      errorFiles,
		WriteErrorFiles: writeErrorFiles,
		EngineStats:     engineStats,
	}, nil
}

func fileContentHash(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	h := sha256.Sum256(data)
	return fmt.Sprintf("%x", h)
}
