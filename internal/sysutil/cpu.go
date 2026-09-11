package sysutil

import (
	"os"
	"runtime"
	"strconv"
)

// CPUBudget returns how many CPUs the indexer may use while leaving headroom for
// interactive work: NumCPU - max(2, NumCPU/4), floored at 1. The Go parse-worker
// pool, LadybugDB's native thread pool, and the ONNX intra-op pool are all
// derived from this single budget so they don't collectively oversubscribe the
// machine. GRAPHIT_MAX_WORKERS overrides it (clamped to [1, NumCPU]).
func CPUBudget() int {
	n := runtime.NumCPU()
	if n < 1 {
		n = 1
	}
	if v := envInt("GRAPHIT_MAX_WORKERS"); v > 0 {
		if v > n {
			return n
		}
		return v
	}
	reserve := n / 4
	if reserve < 2 {
		reserve = 2
	}
	b := n - reserve
	if b < 1 {
		b = 1
	}
	return b
}

func envInt(key string) int {
	s := os.Getenv(key)
	if s == "" {
		return 0
	}
	n, err := strconv.Atoi(s)
	if err != nil || n < 0 {
		return 0
	}
	return n
}
