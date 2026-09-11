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
		w := SafeWorkers(-5)
		if w != sysutil.CPUBudget() {
			t.Errorf("negative input should use CPUBudget()=%d, got %d", sysutil.CPUBudget(), w)
		}
	})
}

func TestParallelForEach(t *testing.T) {
	const n = 1000
	items := make([]int, n)
	for i := range items {
		items[i] = i
	}

	sum, count := 0, 0
	parallelForEach(items, 8,
		func(x int) int { return x * 2 },
		func(r int) { sum += r; count++ },
	)

	if count != n {
		t.Errorf("collected %d results, want %d", count, n)
	}
	want := 0
	for _, x := range items {
		want += x * 2
	}
	if sum != want {
		t.Errorf("sum = %d, want %d", sum, want)
	}
}

func TestParallelForEachEmpty(t *testing.T) {
	called := false
	parallelForEach([]int{}, 4,
		func(x int) int { called = true; return x },
		func(int) { called = true },
	)
	if called {
		t.Error("work/collect must not run for empty input")
	}
}

func TestParallelForEachClampsWorkers(t *testing.T) {
	got := 0
	parallelForEach([]int{1, 2, 3, 4}, 0,
		func(x int) int { return x },
		func(r int) { got += r },
	)
	if got != 10 {
		t.Errorf("got %d, want 10", got)
	}
}
