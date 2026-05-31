//go:build windows

package updater

import (
	"errors"
	"syscall"
)

func isCrossDevice(err error) bool {
	const ERROR_NOT_SAME_DEVICE syscall.Errno = 17
	return errors.Is(err, ERROR_NOT_SAME_DEVICE)
}
