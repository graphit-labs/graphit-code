//go:build windows

package paths

import (
	"fmt"
	"os/exec"
)

func windowsFallbackJunction(source, linkPath string, isDir bool, origErr error) error {
	if !isDir {
		return fmt.Errorf("symlink %s → %s: %w", source, linkPath, origErr)
	}
	cmd := exec.Command("cmd", "/c", "mklink", "/J", linkPath, source)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("mklink /J failed: %s: %w", string(out), origErr)
	}
	return nil
}
