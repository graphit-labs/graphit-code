//go:build windows

package main

import "errors"

func sanitizeInheritedFDs() {}

func execCore(coreBinPath string, args []string, env []string) error {
	return errors.New("exec not supported on Windows")
}
