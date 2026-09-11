//go:build !linux && !darwin && !windows

package sysutil

// LowerPriority is a no-op on platforms without a supported priority API.
func LowerPriority() error { return nil }
