//go:build !linux

package tray

import "os"

func graphicalEnvironment() ([]string, bool) { return os.Environ(), true }
