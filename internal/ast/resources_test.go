package ast

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// TestBoundedDBBufferPool checks the contract rather than an exact number: the
// pool is derived from the machine's effective memory limit (which varies by
// host and container), so the guarantees are that it stays inside
// [floor, ceil] for its role and never inflates a default that is already tiny.
func TestBoundedDBBufferPool(t *testing.T) {
	const gib = uint64(1) << 30

	t.Run("stays within floor and ceiling", func(t *testing.T) {
		for _, def := range []uint64{16 * gib, gib + 512<<20, 400 << 20} {
			for _, readOnly := range []bool{false, true} {
				ceil := dbBufferPoolCeilWrite
				if readOnly {
					ceil = dbBufferPoolCeilRead
				}
				got := boundedDBBufferPool(def, readOnly)
				if got < dbBufferPoolFloor || got > ceil {
					t.Errorf("boundedDBBufferPool(%d, readOnly=%v) = %d, want within [%d,%d]",
						def, readOnly, got, dbBufferPoolFloor, ceil)
				}
			}
		}
	})

	t.Run("tiny default not inflated", func(t *testing.T) {
		const tiny = 128 << 20
		for _, readOnly := range []bool{false, true} {
			if got := boundedDBBufferPool(tiny, readOnly); got != tiny {
				t.Errorf("boundedDBBufferPool(%d, readOnly=%v) = %d, want it left alone",
					uint64(tiny), readOnly, got)
			}
		}
	})

	t.Run("a large machine gives the writer a large pool", func(t *testing.T) {
		t.Setenv("GRAPHIT_DB_BUFFER_MB", "")
		limit := uint64(32 * gib)
		def := uint64(float64(limit) * 0.8)
		got := boundedDBBufferPool(def, false)
		if got < 4*gib {
			t.Errorf("write pool = %d MiB on a %d MiB machine, want >= 4 GiB",
				got>>20, limit>>20)
		}
	})

	t.Run("a read handle stays bounded even on a large machine", func(t *testing.T) {
		t.Setenv("GRAPHIT_DB_BUFFER_MB", "")
		limit := uint64(32 * gib)
		def := uint64(float64(limit) * 0.8)
		got := boundedDBBufferPool(def, true)
		if got > dbBufferPoolCeilRead {
			t.Errorf("read pool = %d MiB, want <= %d MiB",
				got>>20, dbBufferPoolCeilRead>>20)
		}
		if got >= boundedDBBufferPool(def, false) {
			t.Errorf("read pool %d MiB is not smaller than the write pool %d MiB",
				got>>20, boundedDBBufferPool(def, false)>>20)
		}
	})
}

// TestAntlrHeapBudget checks the ANTLR cache budget scales with the machine and
// stays inside its bounds.
func TestAntlrHeapBudget(t *testing.T) {
	got := AntlrHeapBudget()
	t.Logf("AntlrHeapBudget = %d MiB", got>>20)
	if got < antlrHeapFloor || got > antlrHeapCeil {
		t.Errorf("AntlrHeapBudget = %d, want within [%d,%d]", got, antlrHeapFloor, antlrHeapCeil)
	}
}

func TestAntlrHeapBudgetEnvOverride(t *testing.T) {
	t.Setenv("GRAPHIT_ANTLR_HEAP_MB", "321")
	if got := AntlrHeapBudget(); got != 321<<20 {
		t.Errorf("env override = %d, want %d", got, uint64(321)<<20)
	}
}

// TestBoundedDBBufferPoolEnvOverride: the override wins for BOTH roles, because it
// is the escape hatch the buffer-pool error message tells the operator to use.
func TestBoundedDBBufferPoolEnvOverride(t *testing.T) {
	t.Setenv("GRAPHIT_DB_BUFFER_MB", "128")
	for _, readOnly := range []bool{false, true} {
		if got := boundedDBBufferPool(16<<30, readOnly); got != 128<<20 {
			t.Errorf("env override (readOnly=%v) = %d, want %d",
				readOnly, got, uint64(128)<<20)
		}
	}
}

func TestBoundedDBThreads(t *testing.T) {
	budget := uint64(sysutil.CPUBudget())

	if got := boundedDBThreads(1024); got != budget {
		t.Errorf("boundedDBThreads(1024) = %d, want budget %d", got, budget)
	}
	if budget >= 1 {
		if got := boundedDBThreads(1); got != 1 {
			t.Errorf("boundedDBThreads(1) = %d, want 1", got)
		}
	}
	if got := boundedDBThreads(0); got < 1 || got > budget {
		t.Errorf("boundedDBThreads(0) = %d, want in [1,%d]", got, budget)
	}
}

func TestBoundedDBThreadsEnvOverride(t *testing.T) {
	t.Setenv("GRAPHIT_DB_THREADS", "2")
	if got := boundedDBThreads(1024); got != 2 {
		t.Errorf("env override = %d, want 2", got)
	}
}
