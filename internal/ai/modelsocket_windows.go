//go:build windows

package ai

import (
	"fmt"
	"os"
)

// ensurePrivateDir creates dir. On Windows the fallback lives in the user's own
// temporary directory, which other users cannot write to.
func ensurePrivateDir(dir string) error {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return fmt.Errorf("create socket dir: %w", err)
	}
	return nil
}
