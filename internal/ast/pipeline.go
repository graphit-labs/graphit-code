package ast

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

type PipelineOptions struct {
	Workers      int
	IsDepend     bool
	IndexSource  bool
	SkipExternal bool
	CacheDir     string
	ExcludeExts  map[string]bool
	Cluster      string
	ForceRebuild bool
}

type PipelineResult struct {
	TotalFiles  int
	ParsedFiles int
	ParseTime   time.Duration
	WriteTime   time.Duration
	TotalTime   time.Duration

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

	tsParser := &TreeSitterParser{}
	return runFileWorkerPool(ctx, db, writer, abs, tsParser, t0, opts)
}

func runFileWorkerPool(ctx context.Context, db GraphDB, writer *GraphWriter, abs string, parser LanguageParser, t0 time.Time, opts PipelineOptions) (*PipelineResult, error) {
	files, err := collectFiles(abs)
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

	var jsonCache *ShardCache
	if opts.CacheDir != "" {
		var err error
		jsonCache, err = NewShardCache(opts.CacheDir)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  [warn] JSON cache: %v (falling back to full parse)\n", err)
		}
	}

	var changedFiles []string
	var deletedFiles []string
	fileHashes := make(map[string]string, len(files))
	if jsonCache != nil {
		type hashResult struct {
			path string
			hash string
			rel  string
		}
		hashCh := make(chan hashResult, len(files))
		var hashWg sync.WaitGroup
		sem := make(chan struct{}, SafeWorkers(0))

		for _, f := range files {
			hashWg.Add(1)
			go func(path string) {
				defer hashWg.Done()
				sem <- struct{}{}
				defer func() { <-sem }()
				h := fileContentHash(path)
				fAbs, _ := filepath.Abs(path)
				rel := writer.rel(fAbs)
				hashCh <- hashResult{path: path, hash: h, rel: rel}
			}(f)
		}

		go func() {
			hashWg.Wait()
			close(hashCh)
		}()

		for hr := range hashCh {
			fileHashes[hr.path] = hr.hash
			if hr.rel == "" {
				continue
			}
			if jsonCache.HasChanged(hr.rel, hr.hash) {
				changedFiles = append(changedFiles, hr.path)
			}
		}

		liveFiles := make(map[string]bool, len(files))
		for _, f := range files {
			fAbs, _ := filepath.Abs(f)
			liveFiles[writer.rel(fAbs)] = true
		}
		for _, cached := range jsonCache.AllPaths() {
			if !liveFiles[cached] {
				deletedFiles = append(deletedFiles, cached)
				jsonCache.Remove(cached)
			}
		}
	} else {
		changedFiles = files
	}

	if len(changedFiles) == 0 && len(deletedFiles) == 0 && jsonCache != nil && jsonCache.Count() > 0 && !opts.ForceRebuild {

		_ = jsonCache.Save()
		_ = jsonCache.Close()
		totalTime := time.Since(t0)
		return &PipelineResult{
			TotalFiles:  len(files),
			ParsedFiles: 0,
			TotalTime:   totalTime,
			EngineStats: make(map[string]int),
		}, nil
	}

	t1 := time.Now()
	parseOpts := ParseOptions{}

	origStdout := os.Stdout

	dryRun := os.Getenv("AST_DRY_RUN") == "1"

	type result struct {
		path string
		pf   *ParsedFile
		err  error
	}
	results := make(chan result, 64)

	if bp, ok := parser.(BatchParser); ok {

		go func() {
			defer close(results)

			batchInput := make([]BatchFileInput, 0, len(changedFiles))
			for _, f := range changedFiles {
				src, err := os.ReadFile(f)
				if err != nil {
					results <- result{f, nil, fmt.Errorf("read error: %w", err)}
					continue
				}
				batchInput = append(batchInput, BatchFileInput{
					Path:     f,
					Content:  string(src),
					IsDepend: opts.IsDepend,
				})
			}

			batchCh := make(chan BatchResult, 64)
			bp.ParseBatch(batchInput, parseOpts, batchCh)
			for br := range batchCh {
				path := ""
				if br.File != nil {
					path = br.File.Path
				}
				results <- result{path, br.File, br.Err}
			}
		}()
	} else {

		paths := make(chan string)
		go func() {
			for _, f := range changedFiles {
				paths <- f
			}
			close(paths)
		}()

		var wg sync.WaitGroup
		for i := 0; i < opts.Workers; i++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				for path := range paths {
					pf, err := parser.Parse(path, opts.IsDepend, parseOpts)
					results <- result{path, pf, err}
				}
			}()
		}
		go func() {
			wg.Wait()
			close(results)
		}()
	}

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
	var memStats runtime.MemStats
	engineStats := make(map[string]int)

	engineLabel := "tree-sitter"

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

		langKey := engineLabel + ":" + r.pf.Language
		if r.pf.Language == "" {
			langKey = engineLabel + ":unknown"
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
				}
			}
		}

		parsedFilesCount++

		runtime.ReadMemStats(&memStats)
		_, _ = fmt.Fprintf(origStdout, "\r\033[K  › Parsing: %d / %d  [Errors: %d | Mem: %dMB]",
			parsedFilesCount, len(changedFiles), parseErrors+writeErrors,
			memStats.HeapInuse/1024/1024)
	}
	_, _ = fmt.Fprintln(origStdout)

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
			if info, statErr := os.Stat(lb.cfg.DBPath); statErr == nil && info.IsDir() {
				productionDBExists = true
			}
		}

		useIncremental := isLB && productionDBExists && !opts.ForceRebuild &&
			(len(changedFiles)+len(deletedFiles)) < jsonCache.Count()

		var searchIdx *SearchIndex
		if isLB {
			idxPath := lb.cfg.DBPath + ".search.sqlite"
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

			fmt.Fprintf(os.Stderr, "  › Strategy: INCREMENTAL (%d changed + %d deleted of %d total)\n",
				len(changedRels), len(deletedFiles), jsonCache.Count())
			err = IncrementalRebuild(ctx, lb, jsonCache, embCache, changedRels, deletedFiles, opts.Cluster, abs, searchIdx)
		} else {
			fmt.Fprintf(os.Stderr, "  › Strategy: FULL REBUILD (%d files)\n", jsonCache.Count())
			err = RebuildFromJSON(ctx, db, jsonCache, embCache, opts.Cluster, abs)
			if err == nil && searchIdx != nil {
				embLookup := buildEmbLookup(jsonCache, embCache)
				_ = searchIdx.RebuildFromCache(jsonCache, embLookup)
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
		TotalFiles:      len(files),
		ParsedFiles:     parsedFilesCount,
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
