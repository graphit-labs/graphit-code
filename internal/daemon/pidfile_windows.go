//go:build windows

package daemon

import (
	"errors"
	"os"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"golang.org/x/sys/windows"
)

func pidIsAlive(pid int) bool {
	handle, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return false
	}
	defer windows.CloseHandle(handle)

	var code uint32
	if err := windows.GetExitCodeProcess(handle, &code); err != nil {
		return false
	}
	return code == 259
}

func flockExclusive(f *os.File) error {
	ol := &windows.Overlapped{Offset: daemonctl.DaemonPIDLockOffset}
	return windows.LockFileEx(
		windows.Handle(f.Fd()),
		windows.LOCKFILE_EXCLUSIVE_LOCK|windows.LOCKFILE_FAIL_IMMEDIATELY,
		0,
		1, 0,
		ol,
	)
}

func flockRelease(f *os.File) {
	ol := &windows.Overlapped{Offset: daemonctl.DaemonPIDLockOffset}
	_ = windows.UnlockFileEx(
		windows.Handle(f.Fd()),
		0,
		1, 0,
		ol,
	)
}

func flockContended(err error) bool {
	return errors.Is(err, windows.ERROR_LOCK_VIOLATION) || errors.Is(err, windows.ERROR_IO_PENDING)
}
