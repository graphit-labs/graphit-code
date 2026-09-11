//go:build !linux && !darwin && !windows

package sysutil

func physicalMemoryBytes() uint64 { return 0 }

func cgroupMemoryLimitBytes() uint64 { return 0 }
