package ast

import "testing"

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
	cases := []struct{ def, want uint64 }{
		{0, 1},
		{4, 4},
		{8, 8},
		{20, dbThreadCap},
	}
	for _, c := range cases {
		if got := boundedDBThreads(c.def); got != c.want {
			t.Errorf("boundedDBThreads(%d) = %d, want %d", c.def, got, c.want)
		}
	}
}

func TestBoundedDBThreadsEnvOverride(t *testing.T) {
	t.Setenv("GRAPHIT_DB_THREADS", "2")
	if got := boundedDBThreads(20); got != 2 {
		t.Errorf("env override = %d, want 2", got)
	}
}
