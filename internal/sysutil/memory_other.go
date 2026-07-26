//go:build !linux && !darwin && !windows

package sysutil

// physicalMemoryBytes has no implementation on this platform; callers fall back
// to a conservative constant.
func physicalMemoryBytes() uint64 { return 0 }

func cgroupMemoryLimitBytes() uint64 { return 0 }
