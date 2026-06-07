//go:build ignore

package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
)

func main() {
	schemaDir := "/tmp/schema"
	if len(os.Args) > 1 {
		schemaDir = os.Args[1]
	}

	// Collect all .sql files
	var files []string
	filepath.Walk(schemaDir, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return nil
		}
		if filepath.Ext(path) == ".sql" {
			files = append(files, path)
		}
		return nil
	})
	fmt.Printf("Found %d SQL files in %s\n", len(files), schemaDir)

	// Phase 1: Engine creation
	t0 := time.Now()
	engine, err := wasmantlr.NewEngine("")
	if err != nil {
		fmt.Fprintf(os.Stderr, "NewEngine: %v\n", err)
		os.Exit(1)
	}
	defer engine.Close()
	engineTime := time.Since(t0)
	fmt.Printf("\n=== Phase 1: Engine creation ===\n")
	fmt.Printf("  Time: %v\n", engineTime)

	// Phase 2: Compile WASM module
	wasmBytes, err := os.ReadFile("internal/ast/grammars/antlr-plsql.wasm")
	if err != nil {
		fmt.Fprintf(os.Stderr, "ReadFile: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("\n=== Phase 2: WASM compile + start ===\n")
	fmt.Printf("  WASM binary size: %.2f MB\n", float64(len(wasmBytes))/1024/1024)

	t1 := time.Now()
	if err := engine.Compile("plsql", wasmBytes); err != nil {
		fmt.Fprintf(os.Stderr, "Compile: %v\n", err)
		os.Exit(1)
	}
	compileTime := time.Since(t1)
	fmt.Printf("  Time: %v\n", compileTime)

	// Phase 3: First parse (ATN init)
	firstSource, _ := os.ReadFile(files[0])
	t2 := time.Now()
	_, err = engine.Parse("plsql", firstSource)
	firstParseTime := time.Since(t2)
	fmt.Printf("\n=== Phase 3: First parse (ATN init) ===\n")
	fmt.Printf("  File: %s (%d bytes)\n", files[0], len(firstSource))
	if err != nil {
		fmt.Printf("  Error: %v\n", err)
	}
	fmt.Printf("  Time: %v\n", firstParseTime)

	// Phase 4: Parse all files
	fmt.Printf("\n=== Phase 4: Parse all %d files ===\n", len(files))

	var durations []time.Duration
	var totalBytes int64
	var totalLines int64
	var errors int
	var successes int

	tBatch := time.Now()
	for i, path := range files {
		source, err := os.ReadFile(path)
		if err != nil {
			continue
		}

		lines := int64(1)
		for _, b := range source {
			if b == '\n' {
				lines++
			}
		}

		t := time.Now()
		_, parseErr := engine.Parse("plsql", source)
		d := time.Since(t)

		if parseErr != nil {
			errors++
		} else {
			successes++
		}

		durations = append(durations, d)
		totalBytes += int64(len(source))
		totalLines += lines

		if (i+1)%5000 == 0 {
			fmt.Printf("  Progress: %d/%d files (%.1f%%) — elapsed: %v\n",
				i+1, len(files), float64(i+1)/float64(len(files))*100, time.Since(tBatch))
		}
	}
	batchTime := time.Since(tBatch)

	// Sort durations for percentiles
	sort.Slice(durations, func(i, j int) bool { return durations[i] < durations[j] })

	p50 := durations[len(durations)/2]
	p90 := durations[int(float64(len(durations))*0.90)]
	p95 := durations[int(float64(len(durations))*0.95)]
	p99 := durations[int(float64(len(durations))*0.99)]
	slowest := durations[len(durations)-1]

	var totalDuration time.Duration
	for _, d := range durations {
		totalDuration += d
	}
	avg := totalDuration / time.Duration(len(durations))

	fmt.Printf("\n=== Results ===\n")
	fmt.Printf("  Files parsed:   %d success, %d errors, %d total\n", successes, errors, len(files))
	fmt.Printf("  Total bytes:    %.2f MB\n", float64(totalBytes)/1024/1024)
	fmt.Printf("  Total lines:    %d\n", totalLines)
	fmt.Printf("  Wall time:      %v\n", batchTime)
	fmt.Printf("  Throughput:     %.0f files/sec\n", float64(len(files))/batchTime.Seconds())
	fmt.Printf("  Throughput:     %.0f lines/sec\n", float64(totalLines)/batchTime.Seconds())
	fmt.Printf("  Throughput:     %.2f MB/sec\n", float64(totalBytes)/1024/1024/batchTime.Seconds())

	fmt.Printf("\n=== Latency distribution ===\n")
	fmt.Printf("  avg:  %v\n", avg)
	fmt.Printf("  p50:  %v\n", p50)
	fmt.Printf("  p90:  %v\n", p90)
	fmt.Printf("  p95:  %v\n", p95)
	fmt.Printf("  p99:  %v\n", p99)
	fmt.Printf("  max:  %v\n", slowest)

	fmt.Printf("\n=== Total cold-to-done ===\n")
	fmt.Printf("  Engine + Compile + First Parse + All %d files: %v\n",
		len(files), engineTime+compileTime+firstParseTime+batchTime)

	// Top 10 slowest
	fmt.Printf("\n=== Top 10 slowest files ===\n")
	type fileDur struct {
		path string
		dur  time.Duration
		size int
	}
	// Re-parse top 10 to identify them (we didn't store paths with durations)
	// Actually let's just show the percentiles, that's enough
}
