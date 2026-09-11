package daemon

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func launcherStampPath() string {
	return filepath.Join(brand.GlobalDir(), "daemon", "launcher.stamp")
}

func readLauncherStamp() string {
	data, err := os.ReadFile(launcherStampPath())
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}
