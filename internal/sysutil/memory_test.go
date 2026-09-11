package sysutil

import "testing"

func TestMemoryLimitBytes(t *testing.T) {
	got := MemoryLimitBytes()
	t.Logf("MemoryLimitBytes() = %d bytes (%d MiB)", got, got>>20)
	if got == 0 {
		t.Skip("platform reported no memory limit; callers fall back to a constant")
	}
	if got < 64<<20 || got > 64<<40 {
		t.Errorf("implausible memory limit: %d bytes", got)
	}
}

func TestMemoryFraction(t *testing.T) {
	const fallback = 999
	limit := MemoryLimitBytes()

	if got := MemoryFraction(0.25, 8<<30, 16<<30, fallback); limit != 0 {
		floor := uint64(8 << 30)
		if limit < floor {
			floor = limit
		}
		if got < floor {
			t.Errorf("effective floor not honored: got %d, want at least %d", got, floor)
		}
	}
	if got := MemoryFraction(0.25, 1<<20, 2<<20, fallback); limit != 0 && got > 2<<20 {
		t.Errorf("ceiling not honored: got %d", got)
	}
	got := MemoryFraction(0.25, 256<<20, 8<<30, fallback)
	t.Logf("MemoryFraction(0.25) = %d MiB (limit %d MiB)", got>>20, limit>>20)
	if limit != 0 && got > limit {
		t.Errorf("fraction %d exceeds limit %d", got, limit)
	}
}
