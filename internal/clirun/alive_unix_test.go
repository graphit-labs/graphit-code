//go:build !windows

package clirun

import (
	"os"
	"strconv"
	"strings"
	"syscall"
)

func processAlive(pid int) bool {
	if err := syscall.Kill(pid, 0); err != nil {
		return false
	}
	// A killed child of an exited parent may linger as a zombie until it is reaped;
	// it is no longer running.
	if data, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat"); err == nil {
		fields := strings.Fields(string(data))
		if len(fields) > 2 && fields[2] == "Z" {
			return false
		}
	}
	return true
}
