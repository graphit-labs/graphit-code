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

	// The ceiling is per ROLE, because the two roles have opposite failure modes.
	//
	// A WRITE handle is the indexer: it runs the bulk COPY of the whole corpus, and
	// it is short-lived and serialised, because the daemon's heavy pipelines go
	// through sysutil.AcquireHeavy (HeavySlots defaults to 1), so at most one of
	// them grows its pool at a time.
	//
	// The number below was set by something that no longer runs here. The write
	// ceiling was raised from 1 GiB for CREATE_FTS_INDEX, which held its whole term
	// dictionary in the pool; the full-text index has since moved out of this engine
	// and into the SQLite sidecar, so nothing in this process builds one any more.
	// It is left at 8 GiB deliberately and with the reason stated: the pool is a
	// lazily-grown maximum, so an unused headroom costs nothing, and lowering it
	// would be an unmeasured change to the COPY path dressed up as a cleanup. What
	// is gone is the MEASUREMENT that justified it — see boundedDBBufferPool.
	//
	// A READ handle is the opposite: the daemon and the MCP server hold one for
	// hours, and a buffer pool does not give memory back once it has grown, so
	// this ceiling is what bounds their resident size for the whole session.
	// Nothing has measured a read that needs more — the one query that ever
	// exhausted 1 GiB was the explorer's default graph query, and that was fixed
	// by making the query cheap rather than the pool bigger.
	dbBufferPoolCeilWrite uint64 = 8 << 30 // 8 GiB
	dbBufferPoolCeilRead  uint64 = 1 << 30 // 1 GiB

	dbBufferPoolFractionWrite = 0.25
	dbBufferPoolFractionRead  = 0.10

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

// boundedDBBufferPool clamps LadybugDB's buffer-pool ceiling.
//
// def is liblbug's computed default (~80% of PHYSICAL RAM), which is doubly
// wrong for us: it ignores any container/cgroup limit, and two databases are
// open at once during an incremental rebuild. The budget is therefore derived
// from the effective memory limit, falling back to halving def when the platform
// cannot report one. GRAPHIT_DB_BUFFER_MB overrides (in MiB).
//
// HISTORY, because the write ceiling is 8 GiB and nothing measured today explains
// why. It was 1 GiB, on the reasoning that the buffer pool is a lazily-grown
// maximum and the graph store is small, so a low ceiling was "effectively free".
// The first half is true; the second was not, in the arrangement of the time —
// the full-text index lived in this engine, and CREATE_FTS_INDEX holds its whole
// term dictionary in the pool. A corpus outgrew 1 GiB long before the machine
// noticed, and the failure was
//
//	Buffer manager exception: Unable to allocate memory! The buffer pool is full
//	and no memory could be freed!
//
// which reads like the machine is out of memory when it has tens of gigabytes
// free. MEASURED then (liblbug 0.18.2), smallest pool that built all nine indexes:
//
//	400k entities    ~1 GiB — marginal: passed one run, failed the next at se_tri
//	1.0M entities     3 GiB — 1 GiB, 1.5 GiB and 2 GiB each failed, at a different index every time
//
// The full-text index has since moved to the SQLite sidecar, so that consumer is
// gone and this ceiling now bounds the bulk COPY alone — which has never been
// measured against it. Anyone tempted to lower it back should measure the COPY
// path first; the old numbers do not apply, and neither does the old failure.
func boundedDBBufferPool(def uint64, readOnly bool) uint64 {
	if mb := envUint("GRAPHIT_DB_BUFFER_MB"); mb > 0 {
		return mb << 20
	}
	frac, ceil := dbBufferPoolFractionWrite, dbBufferPoolCeilWrite
	if readOnly {
		frac, ceil = dbBufferPoolFractionRead, dbBufferPoolCeilRead
	}
	// Never inflate a machine whose default is already at/below the floor.
	if def <= dbBufferPoolFloor {
		return def
	}
	v := sysutil.MemoryFraction(frac, dbBufferPoolFloor, ceil, def/2)
	if v < dbBufferPoolFloor {
		return dbBufferPoolFloor
	}
	if v > ceil {
		return ceil
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
