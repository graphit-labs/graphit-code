package daemon

import "github.com/graphit-labs/graphit-code/internal/daemonctl"

func EnsureRunning() (started bool, err error) {
	return daemonctl.EnsureRunning()
}
