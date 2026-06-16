package daemon

import "github.com/graphit-labs/graphit-code/internal/daemonctl"

func EnsureRunning() (started bool, err error) {
	pid := NewPIDFile()
	if pid.IsAlive() != nil {
		return false, nil
	}
	return daemonctl.EnsureRunning()
}
