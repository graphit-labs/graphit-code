package ast

import (
	"os"
	"strconv"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// Resource governance for the indexer.
//
// LadybugDB is opened with liblbug's DefaultSystemConfig, whose buffer pool is
// ~80% of system RAM and whose native thread pool is sized to NumCPU. During an
// incremental rebuild TWO databases are open at once (production + a working
// copy), so the raw defaults intend ~160% RAM and 2xNumCPU native C threads
// that GOMAXPROCS cannot rein in — a primary cause of the machine freezing
// during full/incremental runs. These helpers clamp both to a predictable,
// machine-friendly envelope. A fully coordinated cross-subsystem budget (Go
// workers + DB threads + ONNX threads) is a later phase; this only removes the
// unbounded defaults.
const (
	dbBufferPoolFloor uint64 = 256 << 20 // 256 MiB
	dbBufferPoolCeil  uint64 = 1 << 30   // 1 GiB
)

// boundedDBBufferPool clamps LadybugDB's buffer-pool ceiling. The buffer pool is
// a lazily-grown maximum, and the graph store is small, so a bounded ceiling is
// effectively free here while keeping peak RAM predictable. def is liblbug's
// computed default (~80% RAM). GRAPHIT_DB_BUFFER_MB overrides (in MiB).
func boundedDBBufferPool(def uint64) uint64 {
	if mb := envUint("GRAPHIT_DB_BUFFER_MB"); mb > 0 {
		return mb << 20
	}
	// Never inflate a machine whose default is already at/below the floor.
	if def <= dbBufferPoolFloor {
		return def
	}
	v := def / 2
	if v < dbBufferPoolFloor {
		return dbBufferPoolFloor
	}
	if v > dbBufferPoolCeil {
		return dbBufferPoolCeil
	}
	return v
}

// boundedDBThreads caps LadybugDB's native thread pool (default = NumCPU) to the
// shared CPU budget so it does not oversubscribe the machine alongside the Go
// worker pool and the ONNX embedding threads. GRAPHIT_DB_THREADS overrides.
func boundedDBThreads(def uint64) uint64 {
	if n := envUint("GRAPHIT_DB_THREADS"); n > 0 {
		return n
	}
	budget := uint64(sysutil.CPUBudget())
	if def == 0 || def > budget {
		return budget
	}
	return def
}

// envUint parses an unsigned integer from an environment variable, returning 0
// when unset or malformed.
func envUint(key string) uint64 {
	s := os.Getenv(key)
	if s == "" {
		return 0
	}
	n, err := strconv.ParseUint(s, 10, 64)
	if err != nil {
		return 0
	}
	return n
}
