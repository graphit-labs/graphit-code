//go:build windows

package daemonctl

import (
	"errors"
	"os"

	"golang.org/x/sys/windows"
)

// DaemonPIDLockOffset is outside the PID stamp so status readers can inspect it.
const DaemonPIDLockOffset = 1024

func flockProbe(f *os.File) error {
	ol := &windows.Overlapped{Offset: DaemonPIDLockOffset}
	return windows.LockFileEx(
		windows.Handle(f.Fd()),
		windows.LOCKFILE_EXCLUSIVE_LOCK|windows.LOCKFILE_FAIL_IMMEDIATELY,
		0,
		1, 0,
		ol,
	)
}

func flockProbeRelease(f *os.File) {
	ol := &windows.Overlapped{Offset: DaemonPIDLockOffset}
	_ = windows.UnlockFileEx(
		windows.Handle(f.Fd()),
		0,
		1, 0,
		ol,
	)
}

func flockExclusiveBlocking(f *os.File) error {
	ol := &windows.Overlapped{Offset: DaemonPIDLockOffset}
	return windows.LockFileEx(
		windows.Handle(f.Fd()),
		windows.LOCKFILE_EXCLUSIVE_LOCK,
		0,
		1, 0,
		ol,
	)
}

func flockContended(err error) bool {
	return errors.Is(err, windows.ERROR_LOCK_VIOLATION) || errors.Is(err, windows.ERROR_IO_PENDING)
}
