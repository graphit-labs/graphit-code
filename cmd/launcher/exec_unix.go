//go:build !windows

package main

import (
	"os"
	"runtime"
	"strconv"
	"syscall"
)

func sanitizeInheritedFDs() {
	fdDir := "/proc/self/fd"
	if runtime.GOOS == "darwin" {
		fdDir = "/dev/fd"
	}

	entries, err := os.ReadDir(fdDir)
	if err != nil {
		return
	}

	for _, entry := range entries {
		fd, err := strconv.Atoi(entry.Name())
		if err != nil || fd < 3 {
			continue
		}
		syscall.CloseOnExec(fd)
	}
}

func execCore(coreBinPath string, env []string) error {
	argv := append([]string{coreBinPath}, os.Args[1:]...)
	return syscall.Exec(coreBinPath, argv, env)
}
