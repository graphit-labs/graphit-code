//go:build linux

package sysutil

import "golang.org/x/sys/unix"

const (
	ioprioWhoProcess = 1
	ioprioClassIdle  = 3
	ioprioClassShift = 13
)

// LowerPriority best-effort lowers this process's CPU (nice +10) and I/O (IDLE
// class) scheduling priority so background indexing/embedding yields to
// interactive work. Any error is returned for the caller to log; it is never
// fatal (e.g. EPERM inside restrictive containers).
func LowerPriority() error {
	nErr := unix.Setpriority(unix.PRIO_PROCESS, 0, 10)

	ioprio := uintptr(ioprioClassIdle << ioprioClassShift)
	if _, _, errno := unix.Syscall(unix.SYS_IOPRIO_SET, ioprioWhoProcess, 0, ioprio); errno != 0 && nErr == nil {
		return errno
	}
	return nErr
}
