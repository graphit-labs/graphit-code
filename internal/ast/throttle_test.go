package ast

import (
	"runtime"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func TestSafeWorkers(t *testing.T) {
	cpus := runtime.NumCPU()
	maxAllowed := cpus - 1
	if maxAllowed < 1 {
		maxAllowed = 1
	}

	t.Run("zero_uses_cpu_budget", func(t *testing.T) {
		w := SafeWorkers(0)
		if w != sysutil.CPUBudget() {
			t.Errorf("default should equal CPUBudget()=%d, got %d", sysutil.CPUBudget(), w)
		}
		if w < 1 || w > cpus {
			t.Errorf("default %d out of range [1,%d]", w, cpus)
		}
	})

	t.Run("one_returns_one", func(t *testing.T) {
		w := SafeWorkers(1)
		if w != 1 {
			t.Errorf("expected 1, got %d", w)
		}
	})

	t.Run("explicit_within_range", func(t *testing.T) {
		if maxAllowed >= 2 {
			w := SafeWorkers(2)
			if w != 2 {
				t.Errorf("expected 2, got %d", w)
			}
		}
	})

	t.Run("explicit_capped_at_cpus_minus_1", func(t *testing.T) {
		huge := 99999
		w := SafeWorkers(huge)
		if w > maxAllowed {
			t.Errorf("expected capped at %d, got %d", maxAllowed, w)
		}
		if w < 1 {
			t.Errorf("expected >= 1, got %d", w)
		}
	})

	t.Run("negative_treated_as_zero", func(t *testing.T) {
		// Negative value: explicit > 0 is false, so falls through to the default.
		w := SafeWorkers(-5)
		if w != sysutil.CPUBudget() {
			t.Errorf("negative input should use CPUBudget()=%d, got %d", sysutil.CPUBudget(), w)
		}
	})
}
