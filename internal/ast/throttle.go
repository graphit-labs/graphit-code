package ast

import (
	"runtime"

	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// SafeWorkers returns a worker count. An explicit request is honored but capped
// at NumCPU-1. The default (explicit <= 0) is the shared CPU budget
// (sysutil.CPUBudget), which scales with the machine while reserving headroom
// for interactive work — the same budget used for LadybugDB and ONNX threads.
func SafeWorkers(explicit int) int {
	if explicit > 0 {
		max := runtime.NumCPU() - 1
		if max < 1 {
			max = 1
		}
		if explicit > max {
			return max
		}
		return explicit
	}

	return sysutil.CPUBudget()
}
