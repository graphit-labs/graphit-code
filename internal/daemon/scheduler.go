package daemon

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func cronMarker() string {
	return "# " + strings.ToUpper(brand.Brand) + "_DAEMON_SCHEDULER"
}

func resolveExePath() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", fmt.Errorf("finding executable: %w", err)
	}
	exe, err = filepath.EvalSymlinks(exe)
	if err != nil {
		return "", fmt.Errorf("resolving executable symlink: %w", err)
	}
	return exe, nil
}

func IsSchedulerInstalled() bool {
	status := SchedulerStatus()
	return !strings.Contains(status, "not installed")
}
