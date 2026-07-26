//go:build linux

package sysutil

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"golang.org/x/sys/unix"
)

func physicalMemoryBytes() uint64 {
	var si unix.Sysinfo_t
	if err := unix.Sysinfo(&si); err != nil {
		return 0
	}
	unitSize := uint64(si.Unit)
	if unitSize == 0 {
		unitSize = 1
	}
	// Totalram is uint64 on 64-bit arches but uint32 on 386/arm/mips, so it must
	// be converted explicitly for this file to compile on 32-bit Linux.
	return uint64(si.Totalram) * unitSize
}

// cgroupMemoryLimitBytes returns the most restrictive memory limit applying to
// this process, or 0 when unlimited/unavailable. Both cgroup v2 (memory.max) and
// v1 (memory.limit_in_bytes) are handled. For v2 the limit can be set at any
// ancestor of the process's cgroup (a container inside a systemd scope, for
// example), so the whole chain up to the mount root is inspected.
func cgroupMemoryLimitBytes() uint64 {
	const root = "/sys/fs/cgroup"

	data, err := os.ReadFile("/proc/self/cgroup")
	if err != nil {
		return 0
	}

	var limit uint64
	consider := func(v uint64) {
		if v > 0 && (limit == 0 || v < limit) {
			limit = v
		}
	}

	for _, line := range strings.Split(string(data), "\n") {
		// v2: "0::<path>"; v1: "<id>:<controllers>:<path>"
		parts := strings.SplitN(line, ":", 3)
		if len(parts) != 3 {
			continue
		}
		controllers, cgPath := parts[1], parts[2]

		var base string
		var file string
		switch {
		case controllers == "": // cgroup v2
			base = filepath.Join(root, cgPath)
			file = "memory.max"
		case strings.Contains(controllers, "memory"): // cgroup v1
			base = filepath.Join(root, "memory", cgPath)
			file = "memory.limit_in_bytes"
		default:
			continue
		}

		// Walk from the process's cgroup up to the mount root.
		for dir := base; strings.HasPrefix(dir, root); dir = filepath.Dir(dir) {
			consider(readMemLimitFile(filepath.Join(dir, file)))
			if dir == root {
				break
			}
		}
	}
	return limit
}

// readMemLimitFile parses a cgroup memory limit file. "max" (v2) and the v1
// sentinel for unlimited both yield 0.
func readMemLimitFile(path string) uint64 {
	b, err := os.ReadFile(path)
	if err != nil {
		return 0
	}
	s := strings.TrimSpace(string(b))
	if s == "" || s == "max" {
		return 0
	}
	v, err := strconv.ParseUint(s, 10, 64)
	if err != nil {
		return 0
	}
	// cgroup v1 encodes "unlimited" as a huge value (PAGE_COUNTER_MAX scaled);
	// anything at or above 1 PiB is treated as no limit.
	if v >= 1<<50 {
		return 0
	}
	return v
}
