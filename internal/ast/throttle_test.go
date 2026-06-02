package ast

import (
	"runtime"
	"testing"
)

func TestSafeWorkers(t *testing.T) {
	cpus := runtime.NumCPU()
	maxAllowed := cpus - 1
	if maxAllowed < 1 {
		maxAllowed = 1
	}

	t.Run("zero_uses_default", func(t *testing.T) {
		w := SafeWorkers(0)
		if w < 2 {
			t.Errorf("expected >= 2, got %d", w)
		}
		if w > 8 {
			t.Errorf("expected <= 8, got %d", w)
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
		// Negative value: explicit > 0 is false, so falls through to default
		w := SafeWorkers(-5)
		if w < 2 {
			t.Errorf("expected >= 2 for negative input, got %d", w)
		}
	})
}
