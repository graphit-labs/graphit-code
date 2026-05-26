package ast

import (
	"runtime"
)

func SafeWorkers(explicit int) int {
	cpus := runtime.NumCPU()

	if explicit > 0 {

		max := cpus - 1
		if max < 1 {
			max = 1
		}
		if explicit > max {
			return max
		}
		return explicit
	}

	w := cpus / 2
	if w < 2 {
		w = 2
	}
	if w > 8 {
		w = 8
	}
	return w
}
