package sysutil

import "testing"

func TestMemoryLimitBytes(t *testing.T) {
	got := MemoryLimitBytes()
	t.Logf("MemoryLimitBytes() = %d bytes (%d MiB)", got, got>>20)
	if got == 0 {
		t.Skip("platform reported no memory limit; callers fall back to a constant")
	}
	// Sanity: at least 64 MiB, at most 64 TiB.
	if got < 64<<20 || got > 64<<40 {
		t.Errorf("implausible memory limit: %d bytes", got)
	}
}

func TestMemoryFraction(t *testing.T) {
	const fallback = 999
	limit := MemoryLimitBytes()

	// Clamping to the floor and the ceiling must hold regardless of machine size.
	if got := MemoryFraction(0.25, 8<<30, 16<<30, fallback); limit != 0 && got < 8<<30 {
		t.Errorf("floor not honored: got %d", got)
	}
	if got := MemoryFraction(0.25, 1<<20, 2<<20, fallback); limit != 0 && got > 2<<20 {
		t.Errorf("ceiling not honored: got %d", got)
	}
	// A sane fraction lands inside the window and never exceeds the limit.
	got := MemoryFraction(0.25, 256<<20, 8<<30, fallback)
	t.Logf("MemoryFraction(0.25) = %d MiB (limit %d MiB)", got>>20, limit>>20)
	if limit != 0 && got > limit {
		t.Errorf("fraction %d exceeds limit %d", got, limit)
	}
}
