package ast

import (
	"runtime"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// parallelForEach runs work over items using a fixed pool of `workers`
// goroutines, invoking collect once per result. Unlike a goroutine-per-item
// fan-out, memory is O(workers) — a bounded feeder channel and a small result
// buffer — regardless of len(items), which matters on huge repositories
// (e.g. tens of thousands of files). collect runs on the calling goroutine, so
// it may safely mutate shared state without additional synchronization.
func parallelForEach[T any, R any](items []T, workers int, work func(T) R, collect func(R)) {
	if len(items) == 0 {
		return
	}
	if workers < 1 {
		workers = 1
	}

	in := make(chan T)
	out := make(chan R, workers*2)

	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for it := range in {
				out <- work(it)
			}
		}()
	}

	go func() {
		for _, it := range items {
			in <- it
		}
		close(in)
		wg.Wait()
		close(out)
	}()

	for r := range out {
		collect(r)
	}
}

// SafeWorkers returns a worker count. An explicit request is honored but capped
// at NumCPU-1. The default (explicit <= 0) is the shared CPU budget
// (sysutil.CPUBudget), which scales with the machine while reserving headroom
// for interactive work — the same budget used for LadybugDB and ONNX threads.
func SafeWorkers(explicit int) int {
	if explicit > 0 {
		max := runtime.NumCPU() - 1
		if max < 1 {
			max = 1
		}
		if explicit > max {
			return max
		}
		return explicit
	}

	return sysutil.CPUBudget()
}
