package main

import (
	"context"
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

type wasmParser struct {
	rt       wazero.Runtime
	compiled wazero.CompiledModule
	stdinW   *io.PipeWriter
	stdoutR  *io.PipeReader
	cancel   context.CancelFunc
	done     chan struct{}
}

func newWasmParser(wasmBytes []byte, cacheDir string) (*wasmParser, error) {
	ctx := context.Background()
	cfg := wazero.NewRuntimeConfigCompiler()
	cfg = cfg.WithMemoryLimitPages(8192) // 512MB

	if cacheDir != "" {
		cache, err := wazero.NewCompilationCacheWithDir(cacheDir)
		if err != nil {
			return nil, err
		}
		cfg = cfg.WithCompilationCache(cache)
	}

	rt := wazero.NewRuntimeWithConfig(ctx, cfg)
	wasi_snapshot_preview1.Instantiate(ctx, rt)

	compiled, err := rt.CompileModule(ctx, wasmBytes)
	if err != nil {
		rt.Close(ctx)
		return nil, err
	}

	p := &wasmParser{rt: rt, compiled: compiled}
	p.startInstance()
	return p, nil
}

func (p *wasmParser) startInstance() {
	stdinR, stdinW := io.Pipe()
	stdoutR, stdoutW := io.Pipe()

	ctx, cancel := context.WithCancel(context.Background())
	p.stdinW = stdinW
	p.stdoutR = stdoutR
	p.cancel = cancel
	p.done = make(chan struct{})

	go func() {
		defer close(p.done)
		modCfg := wazero.NewModuleConfig().
			WithName("plsql").
			WithStdin(stdinR).
			WithStdout(stdoutW).
			WithStderr(io.Discard).
			WithArgs("plsql")
		p.rt.InstantiateModule(ctx, p.compiled, modCfg)
		stdoutW.Close()
	}()
}

func (p *wasmParser) restart() {
	p.stdinW.Close()
	p.cancel()
	<-p.done
	p.startInstance()
}

func (p *wasmParser) parse(source []byte, timeout time.Duration) ([]byte, error) {
	type result struct {
		data []byte
		err  error
	}
	ch := make(chan result, 1)

	go func() {
		lenBuf := make([]byte, 4)
		binary.BigEndian.PutUint32(lenBuf, uint32(len(source)))
		if _, err := p.stdinW.Write(lenBuf); err != nil {
			ch <- result{nil, err}
			return
		}
		if _, err := p.stdinW.Write(source); err != nil {
			ch <- result{nil, err}
			return
		}
		respLen := make([]byte, 4)
		if _, err := io.ReadFull(p.stdoutR, respLen); err != nil {
			ch <- result{nil, err}
			return
		}
		n := binary.BigEndian.Uint32(respLen)
		if n == 0 || n > 256*1024*1024 {
			ch <- result{nil, fmt.Errorf("invalid response length: %d", n)}
			return
		}
		resp := make([]byte, n)
		if _, err := io.ReadFull(p.stdoutR, resp); err != nil {
			ch <- result{nil, err}
			return
		}
		ch <- result{resp, nil}
	}()

	select {
	case r := <-ch:
		return r.data, r.err
	case <-time.After(timeout):
		return nil, fmt.Errorf("timeout after %v", timeout)
	}
}

func (p *wasmParser) close() {
	p.stdinW.Close()
	p.cancel()
	p.rt.Close(context.Background())
}

func main() {
	schemaDir := "/tmp/schema"
	if len(os.Args) > 1 {
		schemaDir = os.Args[1]
	}

	cacheDir := "/tmp/wazero-plsql-cache"
	os.MkdirAll(cacheDir, 0o755)

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
	fmt.Printf("Compilation cache: %s\n\n", cacheDir)

	wasmBytes, err := os.ReadFile("internal/ast/grammars/antlr-plsql.wasm")
	if err != nil {
		fmt.Fprintf(os.Stderr, "ReadFile: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("WASM binary: %.2f MB\n", float64(len(wasmBytes))/1024/1024)

	t0 := time.Now()
	parser, err := newWasmParser(wasmBytes, cacheDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "newWasmParser: %v\n", err)
		os.Exit(1)
	}
	defer parser.close()
	compileTime := time.Since(t0)
	fmt.Printf("Engine + JIT compile: %v\n\n", compileTime)

	fmt.Printf("=== Parsing %d files ===\n", len(files))

	var durations []time.Duration
	var totalBytes int64
	var totalLines int64
	var successes, errors, timeouts, restarts int
	errByType := map[string]int{} // group errors by parent dir

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
		_, parseErr := parser.parse(source, 120*time.Second)
		d := time.Since(t)

		if parseErr != nil {
			errors++
			// Get parent directory name for grouping
			rel, _ := filepath.Rel(schemaDir, path)
			parts := strings.SplitN(rel, string(os.PathSeparator), 2)
			if len(parts) > 0 {
				errByType[parts[0]]++
			}
			if d >= 119*time.Second {
				timeouts++
			}
			parser.restart()
			restarts++
		} else {
			successes++
		}

		durations = append(durations, d)
		totalBytes += int64(len(source))
		totalLines += lines

		if (i+1)%2000 == 0 {
			elapsed := time.Since(tBatch)
			rate := float64(i+1) / elapsed.Seconds()
			eta := time.Duration(float64(len(files)-i-1) / rate * float64(time.Second))
			fmt.Printf("  Progress: %d/%d (%.1f%%) — %d ok, %d err — %.0f files/sec — ETA: %v\n",
				i+1, len(files), float64(i+1)/float64(len(files))*100,
				successes, errors, rate, eta.Truncate(time.Second))
		}
	}
	batchTime := time.Since(tBatch)

	// Percentiles
	sort.Slice(durations, func(i, j int) bool { return durations[i] < durations[j] })
	p50 := durations[len(durations)/2]
	p90 := durations[int(float64(len(durations))*0.90)]
	p95 := durations[int(float64(len(durations))*0.95)]
	p99 := durations[int(float64(len(durations))*0.99)]
	pMax := durations[len(durations)-1]

	var totalDuration time.Duration
	for _, d := range durations {
		totalDuration += d
	}
	avg := totalDuration / time.Duration(len(durations))

	successRate := float64(successes) / float64(len(files)) * 100

	fmt.Printf("\n╔══════════════════════════════════╗\n")
	fmt.Printf("║         BENCHMARK RESULTS        ║\n")
	fmt.Printf("╠══════════════════════════════════╣\n")
	fmt.Printf("║ Files:     %6d success        ║\n", successes)
	fmt.Printf("║            %6d errors         ║\n", errors)
	fmt.Printf("║            %6d total          ║\n", len(files))
	fmt.Printf("║ Success:   %5.1f%%               ║\n", successRate)
	fmt.Printf("║ Data:      %6.1f MB             ║\n", float64(totalBytes)/1024/1024)
	fmt.Printf("║            %6d lines          ║\n", totalLines)
	fmt.Printf("╠══════════════════════════════════╣\n")
	fmt.Printf("║ JIT compile: %18v ║\n", compileTime.Truncate(time.Millisecond))
	fmt.Printf("║ Parse time:  %18v ║\n", batchTime.Truncate(time.Millisecond))
	fmt.Printf("║ Total:       %18v ║\n", time.Since(t0).Truncate(time.Millisecond))
	fmt.Printf("╠══════════════════════════════════╣\n")
	fmt.Printf("║ Throughput:                      ║\n")
	fmt.Printf("║   %6.0f files/sec               ║\n", float64(len(files))/batchTime.Seconds())
	fmt.Printf("║   %6.0f lines/sec               ║\n", float64(totalLines)/batchTime.Seconds())
	fmt.Printf("║   %6.2f MB/sec                  ║\n", float64(totalBytes)/1024/1024/batchTime.Seconds())
	fmt.Printf("╠══════════════════════════════════╣\n")
	fmt.Printf("║ Latency (per file):              ║\n")
	fmt.Printf("║   avg: %25v ║\n", avg.Truncate(time.Microsecond))
	fmt.Printf("║   p50: %25v ║\n", p50.Truncate(time.Microsecond))
	fmt.Printf("║   p90: %25v ║\n", p90.Truncate(time.Microsecond))
	fmt.Printf("║   p95: %25v ║\n", p95.Truncate(time.Microsecond))
	fmt.Printf("║   p99: %25v ║\n", p99.Truncate(time.Microsecond))
	fmt.Printf("║   max: %25v ║\n", pMax.Truncate(time.Microsecond))
	fmt.Printf("╠══════════════════════════════════╣\n")
	fmt.Printf("║ Restarts: %6d                ║\n", restarts)
	fmt.Printf("║ Timeouts: %6d                ║\n", timeouts)
	fmt.Printf("╚══════════════════════════════════╝\n")

	if len(errByType) > 0 {
		fmt.Printf("\nErrors by type:\n")
		type errCount struct {
			name  string
			count int
		}
		var sorted []errCount
		for k, v := range errByType {
			sorted = append(sorted, errCount{k, v})
		}
		sort.Slice(sorted, func(i, j int) bool { return sorted[i].count > sorted[j].count })
		for _, ec := range sorted {
			fmt.Printf("  %-30s %d\n", ec.name, ec.count)
		}
	}
}
