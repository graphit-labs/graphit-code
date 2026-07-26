package sysutil

// Memory budgeting.
//
// Limits derived from memory must scale with the machine actually running the
// indexer: a fixed ceiling wastes a 128 GB server and thrashes an 8 GB laptop.
// They must also respect a container/cgroup limit when present, because inside a
// container the physical RAM of the host is not what the process may use.

// MemoryLimitBytes returns the memory ceiling this process should budget
// against: the smaller of physical RAM and any cgroup/container limit. It
// returns 0 when the platform provides no usable answer, in which case callers
// must fall back to a conservative constant.
func MemoryLimitBytes() uint64 {
	total := physicalMemoryBytes()
	if cg := cgroupMemoryLimitBytes(); cg > 0 && (total == 0 || cg < total) {
		return cg
	}
	return total
}

// MemoryFraction returns frac of the effective memory limit, clamped to
// [minBytes, maxBytes]. When the limit is unknown it returns fallback.
//
// The result never exceeds the effective limit itself: raising a small
// container's budget to a floor larger than its cgroup limit would hand back a
// number the process can never reach, silently disabling whatever guard depends
// on it.
func MemoryFraction(frac float64, minBytes, maxBytes, fallback uint64) uint64 {
	limit := MemoryLimitBytes()
	if limit == 0 {
		return fallback
	}
	v := uint64(float64(limit) * frac)
	if v < minBytes {
		v = minBytes
	}
	if v > maxBytes {
		v = maxBytes
	}
	// Never promise more than the machine/container actually has.
	if v > limit {
		v = limit
	}
	return v
}
