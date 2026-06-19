//go:build !windows

package sysutil

import (
	"errors"
	"os"
)

func RunLoop(exe string, argv []string, fn func() error) error {
	err := fn()
	if !errors.Is(err, ErrReplace) {
		return err
	}
	return ReplaceProcess(exe, argv, os.Environ())
}
