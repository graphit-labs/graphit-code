//go:build !windows

package paths

import "fmt"

func windowsFallbackJunction(source, linkPath string, isDir bool, origErr error) error {
	return fmt.Errorf("symlink %s → %s: %w", source, linkPath, origErr)
}
