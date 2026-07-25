package sysutil

import (
	"runtime"
	"testing"
)

func TestCPUBudgetLeavesHeadroom(t *testing.T) {
	n := runtime.NumCPU()
	b := CPUBudget()
	if b < 1 {
		t.Fatalf("CPUBudget()=%d, must be >= 1", b)
	}
	if b > n {
		t.Fatalf("CPUBudget()=%d exceeds NumCPU=%d", b, n)
	}
	// On machines with more than a handful of cores it must reserve headroom.
	if n >= 8 && b >= n {
		t.Errorf("CPUBudget()=%d leaves no headroom on a %d-core machine", b, n)
	}
}

func TestCPUBudgetEnvOverride(t *testing.T) {
	n := runtime.NumCPU()

	t.Setenv("GRAPHIT_MAX_WORKERS", "1")
	if got := CPUBudget(); got != 1 {
		t.Errorf("override=1: got %d, want 1", got)
	}

	// Over-large override clamps to NumCPU.
	t.Setenv("GRAPHIT_MAX_WORKERS", "100000")
	if got := CPUBudget(); got != n {
		t.Errorf("override clamp: got %d, want %d", got, n)
	}

	// Malformed override is ignored (falls back to the computed budget).
	t.Setenv("GRAPHIT_MAX_WORKERS", "abc")
	if got := CPUBudget(); got < 1 || got > n {
		t.Errorf("malformed override: got %d, want in [1,%d]", got, n)
	}
}
