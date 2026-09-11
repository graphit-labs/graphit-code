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
		parts := strings.SplitN(line, ":", 3)
		if len(parts) != 3 {
			continue
		}
		controllers, cgPath := parts[1], parts[2]

		var base string
		var file string
		switch {
		case controllers == "":
			base = filepath.Join(root, cgPath)
			file = "memory.max"
		case strings.Contains(controllers, "memory"):
			base = filepath.Join(root, "memory", cgPath)
			file = "memory.limit_in_bytes"
		default:
			continue
		}

		for dir := base; strings.HasPrefix(dir, root); dir = filepath.Dir(dir) {
			consider(readMemLimitFile(filepath.Join(dir, file)))
			if dir == root {
				break
			}
		}
	}
	return limit
}

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
	if v >= 1<<50 {
		return 0
	}
	return v
}
