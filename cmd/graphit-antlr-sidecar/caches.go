package main

import (
	"os"
	"runtime"
	"strconv"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// heapBudget is the Go-heap ceiling above which the ANTLR caches are released.
// It scales with the machine's effective memory (physical RAM or the container's
// cgroup limit, whichever is smaller) instead of being a fixed constant.
// GRAPHIT_ANTLR_HEAP_MB overrides, matching the indexer's knob.
var heapBudget = func() uint64 {
	if s := os.Getenv("GRAPHIT_ANTLR_HEAP_MB"); s != "" {
		if mb, err := strconv.ParseUint(s, 10, 64); err == nil && mb > 0 {
			return mb << 20
		}
	}
	return sysutil.MemoryFraction(0.08, 256<<20, 32<<30, 1<<30)
}()

// releaseCachesUnderPressure drops the accumulated ANTLR caches once the heap
// exceeds the budget. ReadMemStats stops the world, so it runs once per request
// rather than per parse decision — cheap next to an ANTLR parse.
func releaseCachesUnderPressure() {
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	if ms.HeapInuse > heapBudget {
		antlrcommon.ResetAllCaches()
	}
}
