//go:build linux

package ast

import (
	"os"

	"golang.org/x/sys/unix"
)

// reflinkClone performs a copy-on-write clone via the FICLONE ioctl. It succeeds
// only for a regular file on a reflink-capable filesystem (btrfs, or XFS with
// reflink=1); on ext4 and other filesystems the ioctl returns ENOTSUP and the
// caller falls back to a byte copy. Directories are not cloned here (the caller
// copies them recursively).
func reflinkClone(src, dst string) error {
	sfi, err := os.Stat(src)
	if err != nil {
		return err
	}
	if !sfi.Mode().IsRegular() {
		return errReflinkUnsupported
	}

	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()

	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, sfi.Mode().Perm())
	if err != nil {
		return err
	}

	if err := unix.IoctlFileClone(int(out.Fd()), int(in.Fd())); err != nil {
		_ = out.Close()
		_ = os.Remove(dst)
		return err
	}
	return out.Close()
}
