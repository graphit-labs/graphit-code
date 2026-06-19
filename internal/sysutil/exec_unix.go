//go:build !windows

package sysutil

import (
	"os"
	"runtime"
	"strconv"
	"syscall"
)

func SanitizeInheritedFDs() {
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

func ReplaceProcess(argv0 string, argv []string, envv []string) error {
	return syscall.Exec(argv0, argv, envv)
}
