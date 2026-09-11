//go:build darwin

package sysutil

import "golang.org/x/sys/unix"

// LowerPriority best-effort lowers this process's CPU scheduling priority
// (nice +10) so background work yields to interactive use. Any error is returned
// for the caller to log; it is never fatal.
func LowerPriority() error {
	return unix.Setpriority(unix.PRIO_PROCESS, 0, 10)
}
