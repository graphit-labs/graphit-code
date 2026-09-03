package ast

import (
	"os"
	"strconv"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

const (
	dbBufferPoolFloor uint64 = 256 << 20

	dbBufferPoolCeilWrite uint64 = 8 << 30
	dbBufferPoolCeilRead  uint64 = 1 << 30

	dbBufferPoolFractionWrite = 0.25
	dbBufferPoolFractionRead  = 0.10

	antlrHeapFraction = 0.08

	antlrHeapFloor    uint64 = 256 << 20
	antlrHeapCeil     uint64 = 32 << 30
	antlrHeapFallback uint64 = 1 << 30
)

func AntlrHeapBudget() uint64 {
	if mb := envUint("GRAPHIT_ANTLR_HEAP_MB"); mb > 0 {
		return mb << 20
	}
	return sysutil.MemoryFraction(antlrHeapFraction, antlrHeapFloor, antlrHeapCeil, antlrHeapFallback)
}

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
