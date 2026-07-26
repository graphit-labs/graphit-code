//go:build darwin

package sysutil

import "golang.org/x/sys/unix"

func physicalMemoryBytes() uint64 {
	v, err := unix.SysctlUint64("hw.memsize")
	if err != nil {
		return 0
	}
	return v
}

// cgroupMemoryLimitBytes has no macOS equivalent; container limits on macOS are
// enforced inside the Linux VM, where the linux build applies.
func cgroupMemoryLimitBytes() uint64 { return 0 }
