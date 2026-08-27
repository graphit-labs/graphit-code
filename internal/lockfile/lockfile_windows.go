//go:build windows

package lockfile

import (
	"os"
	"unsafe"

	"golang.org/x/sys/windows"
)

func flockTry(f *os.File) error {
	ol := new(windows.Overlapped)
	return windows.LockFileEx(
		windows.Handle(f.Fd()),
		windows.LOCKFILE_EXCLUSIVE_LOCK|windows.LOCKFILE_FAIL_IMMEDIATELY,
		0,
		1, 0,
		(*windows.Overlapped)(unsafe.Pointer(ol)),
	)
}

func flockRelease(f *os.File) {
	ol := new(windows.Overlapped)
	_ = windows.UnlockFileEx(
		windows.Handle(f.Fd()),
		0,
		1, 0,
		(*windows.Overlapped)(unsafe.Pointer(ol)),
	)
}
