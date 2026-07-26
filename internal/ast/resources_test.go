package ast

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// TestBoundedDBBufferPool checks the contract rather than an exact number: the
// pool is derived from the machine's effective memory limit (which varies by
// host and container), so the guarantees are that it stays inside
// [floor, ceil] and never inflates a default that is already tiny.
func TestBoundedDBBufferPool(t *testing.T) {
	const gib = uint64(1) << 30

	t.Run("stays within floor and ceiling", func(t *testing.T) {
		for _, def := range []uint64{16 * gib, gib + 512<<20, 400 << 20} {
			got := boundedDBBufferPool(def)
			if got < dbBufferPoolFloor || got > dbBufferPoolCeil {
				t.Errorf("boundedDBBufferPool(%d) = %d, want within [%d,%d]",
					def, got, dbBufferPoolFloor, dbBufferPoolCeil)
			}
		}
	})

	t.Run("tiny default not inflated", func(t *testing.T) {
		const tiny = 128 << 20 // already below the floor
		if got := boundedDBBufferPool(tiny); got != tiny {
			t.Errorf("boundedDBBufferPool(%d) = %d, want it left alone", uint64(tiny), got)
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

func TestBoundedDBBufferPoolEnvOverride(t *testing.T) {
	t.Setenv("GRAPHIT_DB_BUFFER_MB", "128")
	if got := boundedDBBufferPool(16 << 30); got != 128<<20 {
		t.Errorf("env override = %d, want %d", got, uint64(128)<<20)
	}
}

func TestBoundedDBThreads(t *testing.T) {
	budget := uint64(sysutil.CPUBudget())

	// A NumCPU-sized default (the liblbug default) is clamped to the budget.
	if got := boundedDBThreads(1024); got != budget {
		t.Errorf("boundedDBThreads(1024) = %d, want budget %d", got, budget)
	}
	// A request already at/below the budget is preserved.
	if budget >= 1 {
		if got := boundedDBThreads(1); got != 1 {
			t.Errorf("boundedDBThreads(1) = %d, want 1", got)
		}
	}
	// Result is always a sane, bounded value.
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
