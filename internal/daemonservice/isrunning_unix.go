//go:build !windows

package daemonservice

import (
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

// IsRunning probes the daemon's singleton PID lock.
func IsRunning() (bool, error) {
	return lockfile.IsLocked(filepath.Join(serviceDir(), "daemon.pid"))
}
