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

	// antlrHeapFraction is the share of the machine's effective memory (physical
	// RAM or the container's cgroup limit, whichever is smaller) budgeted for the
	// ANTLR caches. It is a FRACTION rather than a constant so the budget tracks
	// the host: a small container must reset early, a large server should keep
	// its warm caches.
	//
	// Peak process RSS lands at roughly 3x this budget (deserialized ATN, CGO
	// allocations, in-flight trees across workers, GC headroom), so 8% keeps peak
	// usage near a quarter of the machine at any size. Measured on a 35k-file
	// Oracle corpus: a 1.5 GiB budget gave 4m19s at 4.9 GB peak, and 4 GiB gave
	// 3m50s at 8.4 GB peak — more memory buys little speed, so the share is
	// deliberately modest.
	antlrHeapFraction = 0.08

	// antlrHeapFloor keeps the budget usable on tiny hosts; MemoryFraction still
	// caps the result at the effective limit, so a container smaller than this
	// gets its own limit rather than an unreachable floor.
	antlrHeapFloor uint64 = 256 << 20 // 256 MiB
	// antlrHeapCeil is a very high backstop against absurd values; the fraction,
	// not this constant, is what normally decides the budget.
	antlrHeapCeil uint64 = 32 << 30 // 32 GiB
	// antlrHeapFallback applies when the platform cannot report a memory limit.
	antlrHeapFallback uint64 = 1 << 30 // 1 GiB
)

// AntlrHeapBudget is the Go-heap ceiling above which the shared ANTLR DFA /
// prediction-context caches are reset at the next parse barrier.
//
// It scales with the machine (and honors a container/cgroup limit) rather than
// being fixed: measured on a real Oracle corpus the caches plateau per class of
// SQL object (~5 GB across 8k files) and then step up as new constructs appear,
// so a machine with room to spare should keep its warm caches — each reset costs
// real parse time — while a small or containerized machine must reset sooner.
// GRAPHIT_ANTLR_HEAP_MB overrides.
func AntlrHeapBudget() uint64 {
	if mb := envUint("GRAPHIT_ANTLR_HEAP_MB"); mb > 0 {
		return mb << 20
	}
	return sysutil.MemoryFraction(antlrHeapFraction, antlrHeapFloor, antlrHeapCeil, antlrHeapFallback)
}

// boundedDBBufferPool clamps LadybugDB's buffer-pool ceiling. The buffer pool is
// a lazily-grown maximum, and the graph store is small, so a bounded ceiling is
// effectively free here while keeping peak RAM predictable.
//
// def is liblbug's computed default (~80% of PHYSICAL RAM), which is doubly
// wrong for us: it ignores any container/cgroup limit, and two databases are
// open at once during an incremental rebuild. The budget is therefore derived
// from the effective memory limit, falling back to halving def when the platform
// cannot report one. GRAPHIT_DB_BUFFER_MB overrides (in MiB).
func boundedDBBufferPool(def uint64) uint64 {
	if mb := envUint("GRAPHIT_DB_BUFFER_MB"); mb > 0 {
		return mb << 20
	}
	// Never inflate a machine whose default is already at/below the floor.
	if def <= dbBufferPoolFloor {
		return def
	}
	v := sysutil.MemoryFraction(0.10, dbBufferPoolFloor, dbBufferPoolCeil, def/2)
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
