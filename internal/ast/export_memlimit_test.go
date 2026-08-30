package ast

import (
	"math"
	"runtime/debug"
	"testing"
)

// The soft limit is what stops a large export from being OOM-killed, so the one thing it
// must never do is outlive the export or override an operator's own GOMEMLIMIT.
func TestExportMemoryLimitInstallsAndRestores(t *testing.T) {
	if availableMemoryBytes() <= 0 {
		t.Skip("machine memory is not readable here")
	}
	original := debug.SetMemoryLimit(-1)
	t.Cleanup(func() { debug.SetMemoryLimit(original) })

	debug.SetMemoryLimit(math.MaxInt64)
	release := applyExportMemoryLimit(nil)

	during := debug.SetMemoryLimit(-1)
	if during == math.MaxInt64 {
		t.Fatal("no limit was installed for the export")
	}
	if during != exportMemoryLimitBytes() {
		t.Errorf("limit = %d, want %d", during, exportMemoryLimitBytes())
	}

	release()
	if got := debug.SetMemoryLimit(-1); got != math.MaxInt64 {
		t.Errorf("limit after release = %d, want it lifted", got)
	}
}

func TestExportMemoryLimitLeavesAnExplicitLimitAlone(t *testing.T) {
	original := debug.SetMemoryLimit(-1)
	t.Cleanup(func() { debug.SetMemoryLimit(original) })

	const operatorLimit = 3 << 30
	debug.SetMemoryLimit(operatorLimit)

	release := applyExportMemoryLimit(nil)
	if got := debug.SetMemoryLimit(-1); got != operatorLimit {
		t.Errorf("limit = %d, want the operator's %d untouched", got, operatorLimit)
	}
	release()
	if got := debug.SetMemoryLimit(-1); got != operatorLimit {
		t.Errorf("limit after release = %d, want the operator's %d untouched", got, operatorLimit)
	}
}

// A nested rebuild must not lift the limit off the one that is still running.
func TestExportMemoryLimitIsHeldUntilTheLastReleaser(t *testing.T) {
	if availableMemoryBytes() <= 0 {
		t.Skip("machine memory is not readable here")
	}
	original := debug.SetMemoryLimit(-1)
	t.Cleanup(func() { debug.SetMemoryLimit(original) })

	debug.SetMemoryLimit(math.MaxInt64)
	first := applyExportMemoryLimit(nil)
	second := applyExportMemoryLimit(nil)

	first()
	if got := debug.SetMemoryLimit(-1); got == math.MaxInt64 {
		t.Fatal("the limit was lifted while a rebuild was still holding it")
	}
	second()
	if got := debug.SetMemoryLimit(-1); got != math.MaxInt64 {
		t.Errorf("limit after the last release = %d, want it lifted", got)
	}
}
