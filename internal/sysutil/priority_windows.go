//go:build windows

package sysutil

import "golang.org/x/sys/windows"

// LowerPriority best-effort lowers this process's scheduling priority class to
// BELOW_NORMAL so background work yields to interactive use. Any error is
// returned for the caller to log; it is never fatal.
func LowerPriority() error {
	return windows.SetPriorityClass(windows.CurrentProcess(), windows.BELOW_NORMAL_PRIORITY_CLASS)
}
