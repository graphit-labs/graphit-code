//go:build windows

package daemonservice

import (
	"errors"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"golang.org/x/sys/windows"
)

// IsRunning probes the same byte range used by daemon.PIDFile on Windows.
func IsRunning() (bool, error) {
	f, err := os.OpenFile(filepath.Join(serviceDir(), "daemon.pid"), os.O_RDWR, 0)
	if os.IsNotExist(err) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	defer f.Close()
	ol := &windows.Overlapped{Offset: lockfile.DaemonPIDLockOffset}
	err = windows.LockFileEx(windows.Handle(f.Fd()), windows.LOCKFILE_EXCLUSIVE_LOCK|windows.LOCKFILE_FAIL_IMMEDIATELY, 0, 1, 0, ol)
	if err != nil {
		if errors.Is(err, windows.ERROR_LOCK_VIOLATION) || errors.Is(err, windows.ERROR_IO_PENDING) {
			return true, nil
		}
		return false, err
	}
	if err := windows.UnlockFileEx(windows.Handle(f.Fd()), 0, 1, 0, ol); err != nil {
		return false, err
	}
	return false, nil
}
