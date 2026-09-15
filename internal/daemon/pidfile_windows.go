//go:build windows

package daemon

import (
	"errors"
	"os"
	"unsafe"

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

func flockContended(err error) bool {
	return errors.Is(err, windows.ERROR_LOCK_VIOLATION) || errors.Is(err, windows.ERROR_IO_PENDING)
}

// LockFileEx denies reads of byte zero through other handles, so publish the
// diagnostic PID separately while keeping the old lock byte for compatibility.
func writePIDMetadata(path, content string) error {
	return os.WriteFile(path+".info", []byte(content), 0o600)
}
func readPIDMetadata(path string) ([]byte, error) { return os.ReadFile(path + ".info") }
func clearPIDMetadata(path string)                { _ = os.Remove(path + ".info") }
