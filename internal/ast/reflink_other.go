//go:build !linux

package ast

// reflinkClone is a no-op on non-Linux platforms; CopyDBDir falls back to a
// portable native byte copy. (macOS clonefile could be added later via
// golang.org/x/sys/unix.Clonefile; Windows has no reflink for our use.)
func reflinkClone(_, _ string) error {
	return errReflinkUnsupported
}
