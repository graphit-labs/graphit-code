package ast

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func TestBoundedDBBufferPool(t *testing.T) {
	const gib = uint64(1) << 30
	cases := []struct {
		name string
		def  uint64
		want uint64
	}{
		{"huge default clamped to ceil", 16 * gib, dbBufferPoolCeil},
		{"half stays under ceil", gib + 512<<20, (gib + 512<<20) / 2},
		{"tiny default not inflated", 128 << 20, 128 << 20},
		{"half below floor raised to floor", 400 << 20, dbBufferPoolFloor},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := boundedDBBufferPool(c.def); got != c.want {
				t.Errorf("boundedDBBufferPool(%d) = %d, want %d", c.def, got, c.want)
			}
		})
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
