//go:build !windows

package ai

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"syscall"
)

// ensurePrivateDir creates dir for this user only, or accepts it when it already is.
// In a shared temporary directory another user could create the directory first and
// swap the socket inside it, so a directory owned by someone else, or a symlink, is
// refused rather than used.
func ensurePrivateDir(dir string) error {
	if err := os.Mkdir(dir, 0o700); err != nil && !errors.Is(err, fs.ErrExist) {
		return fmt.Errorf("create socket dir: %w", err)
	}
	info, err := os.Lstat(dir)
	if err != nil {
		return fmt.Errorf("inspect socket dir: %w", err)
	}
	if !info.IsDir() {
		return fmt.Errorf("socket dir %s is not a directory", dir)
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != currentUID() {
		return fmt.Errorf("socket dir %s belongs to another user", dir)
	}
	if info.Mode().Perm()&0o077 != 0 {
		if err := os.Chmod(dir, 0o700); err != nil {
			return fmt.Errorf("restrict socket dir: %w", err)
		}
	}
	return nil
}
