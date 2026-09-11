package main

import (
	"os"
	"runtime"
	"strconv"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

var heapBudget = func() uint64 {
	if s := os.Getenv("GRAPHIT_ANTLR_HEAP_MB"); s != "" {
		if mb, err := strconv.ParseUint(s, 10, 64); err == nil && mb > 0 {
			return mb << 20
		}
	}
	return sysutil.MemoryFraction(0.08, 256<<20, 32<<30, 1<<30)
}()

func releaseCachesUnderPressure() {
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	if ms.HeapInuse > heapBudget {
		antlrcommon.ResetAllCaches()
	}
}
