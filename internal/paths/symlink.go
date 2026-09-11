package paths

import "os"

func SafeSymlink(source, linkPath string) error {

	if info, err := os.Lstat(linkPath); err == nil {
		if info.Mode()&os.ModeSymlink != 0 {
			_ = os.Remove(linkPath)
		} else if info.IsDir() {
			_ = os.RemoveAll(linkPath)
		} else {
			_ = os.Remove(linkPath)
		}
	}

	isDir := false
	if info, err := os.Stat(source); err == nil {
		isDir = info.IsDir()
	}

	if err := os.Symlink(source, linkPath); err != nil {

		return windowsFallbackJunction(source, linkPath, isDir, err)
	}
	return nil
}
