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
	"sync/atomic"
	"time"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// antlrCacheCheckInterval is how many files are parsed between memory checks.
// A check costs a runtime.ReadMemStats (a brief stop-the-world), so it is
// amortized over this many files rather than run after every parse. It is no
// longer a barrier: the reset is made safe by antlrcommon's RWMutex, not by
// draining the pool.
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
	ClusterPathMap   map[string]string
	ForceRebuild     bool
	// ReverseEdges controls whether the local bundle carries <TYPE>_REVERSE
	// CSRs, mirroring hub.icebug.reverse_edges — the bundle is the published
	// artifact, so the local build is what the config decides. The zero value
	// means "the config default", which is ON (the config default), so a caller
	// wanting them off passes false after resolving the config.
	ReverseEdges *bool
	Logger       *slog.Logger
	OnProgress   func(phase string, current, total, errors int)

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

	// SearchIndexRebuilt reports that nothing was reparsed and nothing was written to the
	// graph, but the search index was replayed from the shard cache because it was missing
	// or empty. Distinguished from a graph rebuild so the CLI can say which half it repaired.
	SearchIndexRebuilt bool

	ErrorCount      int
	TimeoutCount    int
	EmptyCount      int
	WriteErrorCount int
	EmptyFiles      []string
	ErrorFiles      []string
	WriteErrorFiles []string

	EngineStats map[string]int
}

// resolveClusterForPath returns the cluster name for a given file path based on the
// cluster path map. It finds the most specific (longest) matching prefix.
// If no match is found, returns the default cluster (may be empty).
func resolveClusterForPath(filePath, rootPath string, clusterPathMap map[string]string, defaultCluster string) string {
	if len(clusterPathMap) == 0 {
		return defaultCluster
	}
	// Get relative path from root for prefix matching
	relPath, err := filepath.Rel(rootPath, filePath)
	if err != nil {
		relPath = filePath
	}
	relPath = filepath.ToSlash(relPath)
	if !strings.HasSuffix(relPath, "/") {
		relPath += "/"
	}

	bestMatch := ""
	bestCluster := ""
	for prefix, cluster := range clusterPathMap {
		if strings.HasPrefix(relPath, prefix) {
			if len(prefix) > len(bestMatch) {
				bestMatch = prefix
				bestCluster = cluster
			}
		}
	}
	if bestCluster != "" {
		return bestCluster
	}
	return defaultCluster
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

	discover := func() ([]string, error) {
		found, err := collectFiles(abs)
		if err != nil {
			return nil, fmt.Errorf("discover files: %w", err)
		}
		if len(opts.ExcludeExts) == 0 {
			return found, nil
		}
		var filtered []string
		for _, f := range found {
			ext := strings.ToLower(filepath.Ext(f))
			if !opts.ExcludeExts[ext] {
				filtered = append(filtered, f)
			}
		}
		return filtered, nil
	}

	tDiscover := time.Now()
	var files []string
	if !scoped {
		var err error
		if files, err = discover(); err != nil {
			return nil, err
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
		} else {
			jsonCache.SetRoot(abs)
		}
	}

	// A scoped run indexes the files a watcher named and trusts the cache to hold the
	// rest of the project. An empty cache voids that premise, and the consequence is
	// not a slow run — it is data loss: the rebuild below builds the WHOLE graph from
	// the cache, so one changed file becomes a one-file graph, and the swap publishes
	// it over a complete one. Observed on this repository, in the daemon log:
	//
	//	strategy selected type=full-rebuild files=1
	//	cache loaded files=1
	//	COPY complete nodes=1 edges=0
	//	swapping DB mode=atomic
	//
	// A bump of shardCacheVersion is the ordinary way to arrive here — it discards the
	// manifest by design — and a deleted cache directory or a fresh clone does the same.
	// Discovery costs one slow pass; publishing a one-file graph costs the graph.
	if scoped && jsonCache != nil && jsonCache.Count() == 0 {
		// Only fall back to full discovery for incremental runs of a store that
		// ALREADY EXISTS: the cache being empty is the anomaly, not the touched
		// files. Don't fall back for a full index (reset/reindex) or a fresh
		// store, where "unknown files" is genuinely the answer.
		dbExists := false
		if lb, ok := db.(*LadybugBackend); ok {
			if _, err := os.Stat(lb.cfg.IcebugDir); err == nil {
				dbExists = true
			}
		}
		if !opts.ForceRebuild && dbExists {
			logger.Warn("scoped run with an empty parse cache — falling back to full discovery, "+
				"because rebuilding from it would publish a graph holding only the named files",
				"changed", len(opts.ChangedPaths), "deleted", len(opts.DeletedPaths))
			found, err := discover()
			if err != nil {
				return nil, err
			}
			files = found
			scoped = false
			opts.ChangedPaths = nil
			opts.DeletedPaths = nil
		}
	}

	tHash := time.Now()
	var changedFiles []string
	var deletedFiles []string
	fileHashes := make(map[string]string, len(files))

	// Build maps for correct cache keys:
	// 1. absolute path -> relative path (for exact matches)
	// 2. basename -> relative path (fallback for when parser only gives basename)
	absToRel := make(map[string]string)
	baseToRel := make(map[string]string)
	for _, rel := range opts.ChangedPaths {
		if absPath, err := filepath.Abs(rel); err == nil {
			absToRel[absPath] = rel
		}
		baseToRel[filepath.Base(rel)] = rel
	}

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

		deletedFiles = append(deletedFiles, pruneVanished(jsonCache, liveFiles)...)

		// mtimeUnchanged files: add their paths to fileHashes using cached hash.
		for _, fi := range mtimeUnchanged {
			fileHashes[fi.path] = jsonCache.GetHash(fi.rel)
		}
	} else {
		// Full rebuild or no cache: skip hashing, treat all files as changed.
		changedFiles = files

		// The prune runs here too, and it is the only thing that removes a file that
		// left the disk: both the graph and the search index are rebuilt from this
		// cache, so a shard left in it is republished into both.
		if jsonCache != nil {
			deletedFiles = append(deletedFiles, pruneVanished(jsonCache, relSet(writer, files))...)
		}
	}
	hashTime := time.Since(tHash)

	// In scoped mode the tree was never walked, so the corpus size comes from the
	// parse cache rather than from a file listing.
	totalFiles := len(files)
	if scoped && jsonCache != nil {
		totalFiles = jsonCache.Count()
	}

	// graphPresent gates the shortcut below. "Nothing changed" is only a reason
	// to skip the write when there is something to skip TO: with the graph gone
	// and the parse cache current, this returned "N files up to date (no changes
	// detected)" and exited, leaving no database behind and reporting success
	// over it. Verified by moving the database aside and re-running.
	//
	// It is also the cheap way to ask for a rebuild from cache: delete the
	// database, keep the shards, and the write below replays them — 8.3 s here
	// against a full reparse, and ~95 s against 16 minutes on a 36k-file repo.
	graphPresent := true
	storeDir := ""
	if lb, ok := db.(*LadybugBackend); ok {
		storeDir = lb.cfg.StoreDir
		if _, statErr := os.Stat(lb.cfg.IcebugDir); statErr != nil {
			graphPresent = false
		}
	}

	if graphPresent && len(changedFiles) == 0 && len(deletedFiles) == 0 && jsonCache != nil && jsonCache.Count() > 0 && !opts.ForceRebuild {

		// The SEARCH index gets the same question the graph just got, and for the same
		// reason: skipping the write is only safe when both halves of the store are there
		// to skip to. A store indexed by an older build has a search directory that
		// exists and holds nothing, and until this check it took the shortcut above and
		// reported "N files up to date" over a search that answered nothing at all.
		//
		// os.Stat cannot answer it — OpenSearchIndex CREATES what it opens, so the
		// directory's existence is evidence of nothing. SearchIndexBuilt counts rows.
		//
		// The repair replays the shards rather than reparsing: the parse cache is current
		// by definition here, which is what made this branch reachable.
		if storeDir != "" && !SearchIndexBuilt(ctx, storeDir) {
			var embCache *ShardEmbCache
			if ec, err := NewShardEmbCache(opts.CacheDir, jsonCache); err == nil {
				embCache = ec
				defer func() { _ = ec.Close() }()
			}
			t1 := time.Now()
			if err := BuildSearchIndexFor(ctx, storeDir, jsonCache, embCache); err != nil {
				_ = jsonCache.Save()
				_ = jsonCache.Close()
				return nil, fmt.Errorf("rebuild search index from cache: %w", err)
			}
			writeTime := time.Since(t1)

			_ = jsonCache.Save()
			_ = jsonCache.Close()
			return &PipelineResult{
				TotalFiles:         totalFiles,
				ParsedFiles:        0,
				DiscoverTime:       discoverTime,
				HashTime:           hashTime,
				WriteTime:          writeTime,
				TotalTime:          time.Since(t0),
				SearchIndexRebuilt: true,
				EngineStats:        make(map[string]int),
			}, nil
		}

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

	// Mirrored into an atomic because the parsers poll this from inside a cgo
	// callback, many times per file: context.cancelCtx.Err takes a mutex on
	// every call, which is the wrong thing to put on that path. The watcher is
	// stopped on return so a long-lived daemon does not accumulate one goroutine
	// per pipeline run.
	var cancelled atomic.Bool
	stopWatch := make(chan struct{})
	defer close(stopWatch)
	go func() {
		select {
		case <-ctx.Done():
			cancelled.Store(true)
		case <-stopWatch:
		}
	}()

	parseOpts := ParseOptions{
		IndexSource: opts.IndexSource,
		Cancelled:   cancelled.Load,
	}

	dryRun := os.Getenv("AST_DRY_RUN") == "1"

	type result struct {
		path string
		pf   *ParsedFile
		err  error
	}
	results := make(chan result, 64)

	// One continuous worker pool over every file. ANTLR's package-level DFA /
	// prediction-context caches grow with every newly seen input pattern and are
	// never evicted (~2 MB per PL/SQL file measured), so a large repository still
	// needs a periodic reset — but the reset no longer needs a barrier to be safe.
	// ResetAntlrCaches takes antlrcommon's exclusive lock, and every native
	// grammar driver already holds the matching read lock for the duration of a
	// parse, so the mutex alone excludes in-flight parses.
	//
	// This used to run in chunks of antlrCacheCheckInterval files with a
	// wg.Wait() between them, which made every chunk cost its SLOWEST FILE while
	// the other workers sat idle. On a corpus whose file sizes span three orders
	// of magnitude that tail dominates: measured here, one 704 KB PL/SQL
	// procedure takes 24.3 s to parse on its own, and the handful of files that
	// size are adjacent in walk order, so they landed in the same chunk and
	// stalled it with N-1 workers doing nothing.
	go func() {
		defer close(results)

		paths := make(chan string)
		go func() {
			defer close(paths)
			for _, f := range changedFiles {
				// Unbuffered: without the ctx arm this blocks forever once the
				// workers stop reading, and the cancellation deadlocks instead
				// of completing.
				select {
				case paths <- f:
				case <-ctx.Done():
					return
				}
			}
		}()

		// Files parsed since the last heap check. Checking costs a
		// runtime.ReadMemStats (a brief stop-the-world), so it is amortized over
		// antlrCacheCheckInterval files rather than run per file.
		var sinceCheck atomic.Int64

		var wg sync.WaitGroup
		for i := 0; i < opts.Workers; i++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				wp := NewCompositeParser(abs, opts.GrammarOverrides)
				for path := range paths {
					if ctx.Err() != nil {
						return
					}
					pf, err := wp.Parse(path, opts.IsDepend, parseOpts)
					select {
					case results <- result{path, pf, err}:
					case <-ctx.Done():
						return
					}

					// Safe here and not mid-parse: Parse has returned, so this
					// goroutine holds no read lock and cannot deadlock against
					// the write lock it is about to ask for.
					if sinceCheck.Add(1) >= int64(antlrCacheCheckInterval) {
						sinceCheck.Store(0)
						// Only reset under real pressure — a warm DFA is worth
						// keeping, and each reset costs a partial re-warm.
						if heap, over := antlrCachePressure(); over {
							ResetAntlrCaches()
							logger.Debug("antlr caches reset", "heap_mb", heap>>20)
						}
					}
				}
			}()
		}
		wg.Wait()

		// The caches are package-level and stay LIVE (not garbage), so a
		// long-lived daemon would otherwise hold the whole budget until the
		// process exits.
		if heap, over := antlrCachePressure(); over {
			ResetAntlrCaches()
			logger.Debug("antlr caches reset", "heap_mb", heap>>20, "final", true)
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
			// Resolve cluster for this specific file
			fileCluster := resolveClusterForPath(r.pf.Path, abs, opts.ClusterPathMap, opts.Cluster)
			// Fix the Path on ParsedFile to use the correct relative path from ChangedPaths
			// The parser may only set the basename; we need the correct relative path for cache keys
			fAbs, _ := filepath.Abs(r.pf.Path)
			correctRelPath := absToRel[fAbs]
			if correctRelPath == "" {
				correctRelPath = baseToRel[filepath.Base(fAbs)]
			}
			if correctRelPath != "" {
				// Set the correct absolute path so ConvertToCache computes the right relPath
				r.pf.Path = filepath.Join(abs, correctRelPath)
			}
			fileCluster = resolveClusterForPath(r.pf.Path, abs, opts.ClusterPathMap, opts.Cluster)
			entry := ConvertToCache(r.pf, abs, opts.IndexSource, fileCluster)
			if entry != nil {
				relPath := correctRelPath
				if relPath == "" {
					fAbs, _ := filepath.Abs(r.pf.Path)
					relPath = writer.rel(fAbs)
				}
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
			// A failed flush loses parsed work that the caller believes is safely on
			// disk; the next run re-parses those files and nothing says why.
			if err := jsonCache.FlushDirty(); err != nil {
				logger.Error("parse cache flush failed; parsed files may be re-parsed next run",
					"parsed", parsedFilesCount, "error", err)
			}
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

		// The write is a single call into the rebuild and reports nothing from
		// inside it — on a 36k-file repository that was 424 s of silence after
		// the last parse line, which reads as a hang. At minimum, say that the
		// phase changed and how much work it has.
		if opts.OnProgress != nil {
			opts.OnProgress("writing", 0, jsonCache.Count(), parseErrors+writeErrors)
		}

		// Local graph is icebug filesystem in-memory – no file DB, no swap. A small delta
		// rewrites only the affected Parquets; otherwise the bundle is rebuilt in full.
		changedRels := make([]string, 0, len(changedFiles))
		for _, f := range changedFiles {
			fAbs, _ := filepath.Abs(f)
			if rel := writer.rel(fAbs); rel != "" {
				changedRels = append(changedRels, rel)
			}
		}
		bundleDir := store.ASTProjectIcebugDir(abs)
		if lb, ok := db.(*LadybugBackend); ok && lb.cfg.IcebugDir != "" {
			bundleDir = lb.cfg.IcebugDir
		}
		// ForceRebuild deliberately distrusts the cache: it must never take the
		// delta path, because a delta's premise is that the untouched shards are
		// current — exactly what ForceRebuild refuses to assume.
		doIncremental := !opts.ForceRebuild &&
			(len(changedRels)+len(deletedFiles) > 0) &&
			(len(changedRels)+len(deletedFiles) < jsonCache.Count()/5)
		if doIncremental {
			if _, statErr := os.Stat(bundleDir); statErr != nil {
				doIncremental = false
			}
		}
		logger.Info("strategy selected", "type", "icebug-rebuild",
			"incremental", doIncremental, "changed", len(changedRels),
			"deleted", len(deletedFiles), "files", jsonCache.Count())
		reverseEdges := true
		if opts.ReverseEdges != nil {
			reverseEdges = *opts.ReverseEdges
		}
		var err error
		if doIncremental {
			err = rebuildIcebugFromCacheWithDelta(ctx, jsonCache, changedRels, deletedFiles, opts.Cluster, abs, opts.Logger, true, bundleDir, reverseEdges)
		} else {
			err = RebuildIcebugFromCacheWithReverse(ctx, jsonCache, opts.Cluster, abs, opts.Logger, bundleDir, reverseEdges)
		}
		if err == nil {
			// Search sidecar (LanceDB): incremental where small, full otherwise.
			//
			// The embedding cache is read here, never written: a rebuild drops the entity
			// table, and without replaying the vectors it already holds every rebuild would
			// re-run the model over the whole corpus.
			var embCache *ShardEmbCache
			if opts.CacheDir != "" {
				if ec, ecErr := NewShardEmbCache(opts.CacheDir, jsonCache); ecErr == nil {
					embCache = ec
				}
			}
			if lb, ok := db.(*LadybugBackend); ok {
				idx, oerr := OpenSearchIndex(ctx, lb.cfg.StoreDir)
				if oerr != nil {
					err = fmt.Errorf("open search index: %w", oerr)
				} else {
					idx.Logger = opts.Logger
					if doIncremental {
						if serr := idx.UpdateIncremental(ctx, jsonCache, changedRels, deletedFiles, BuildEmbLookup(jsonCache, embCache)); serr != nil {
							err = fmt.Errorf("search index incremental: %w", serr)
						}
					} else if serr := idx.RebuildFromCache(ctx, jsonCache, BuildEmbLookup(jsonCache, embCache)); serr != nil {
						err = fmt.Errorf("search index rebuild: %w", serr)
					}
					if err == nil {
						idx.Maintain(ctx)
					}
					_ = idx.Close()
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

// pruneVanished drops every cached shard whose file is no longer among the live
// ones and returns their repo-relative paths.
//
// NOTE: only safe when live covers the whole corpus. In scoped mode the tree is
// never walked, so the caller names the deletions instead.
func pruneVanished(cache *ShardCache, live map[string]bool) []string {
	var gone []string
	for _, cached := range cache.AllPaths() {
		if !live[cached] {
			gone = append(gone, cached)
			cache.Remove(cached)
		}
	}
	return gone
}

func relSet(writer *GraphWriter, files []string) map[string]bool {
	live := make(map[string]bool, len(files))
	for _, path := range files {
		abs, err := filepath.Abs(path)
		if err != nil {
			continue
		}
		if rel := writer.rel(abs); rel != "" {
			live[rel] = true
		}
	}
	return live
}

func fileContentHash(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return contentHashOf(data)
}

// contentHashOf is the hash a shard is keyed by, taken from bytes already in hand so a
// caller that needs both the hash and the content does not read the file twice.
func contentHashOf(data []byte) string {
	h := sha256.Sum256(data)
	return fmt.Sprintf("%x", h)
}
