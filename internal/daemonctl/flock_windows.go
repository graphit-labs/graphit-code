//go:build windows

package daemonctl

import (
	"os"
	"unsafe"

	"golang.org/x/sys/windows"
)

func flockProbe(f *os.File) error {
	ol := new(windows.Overlapped)
	return windows.LockFileEx(
		windows.Handle(f.Fd()),
		windows.LOCKFILE_EXCLUSIVE_LOCK|windows.LOCKFILE_FAIL_IMMEDIATELY,
		0,
		1, 0,
		(*windows.Overlapped)(unsafe.Pointer(ol)),
	)
}

func flockProbeRelease(f *os.File) {
	ol := new(windows.Overlapped)
	_ = windows.UnlockFileEx(
		windows.Handle(f.Fd()),
		0,
		1, 0,
		(*windows.Overlapped)(unsafe.Pointer(ol)),
	)
}
