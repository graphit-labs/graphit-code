//go:build !windows

package daemonctl

import (
	"os"
	"syscall"
)

func flockProbe(f *os.File) error {
	return syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB)
}

func flockProbeRelease(f *os.File) {
	_ = syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
}
