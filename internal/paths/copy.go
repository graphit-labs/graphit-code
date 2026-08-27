package paths

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
)

func SafeCopyDir(source, dest string) error {
	srcInfo, err := os.Stat(source)
	if err != nil {
		return fmt.Errorf("source %s: %w", source, err)
	}
	if !srcInfo.IsDir() {
		return fmt.Errorf("source %s is not a directory", source)
	}

	removeIfSymlink(dest)
	if err := os.RemoveAll(dest); err != nil {
		return fmt.Errorf("removing dest %s: %w", dest, err)
	}

	return copyDirRecursive(source, dest)
}

// SyncCopyDir mirrors source onto dest, adding, overwriting and deleting so dest
// ends up matching source.
func SyncCopyDir(source, dest string) error {
	return SyncCopyDirExcept(source, dest, nil)
}

// SyncCopyDirExcept is SyncCopyDir with entries the caller does not want mirrored.
//
// skip receives each entry's path relative to source, with forward slashes on every
// platform so a caller's rule reads the same on Windows as on Unix. A skipped entry
// is neither copied into dest nor deleted from it — it is simply not this mirror's
// business.
func SyncCopyDirExcept(source, dest string, skip func(rel string) bool) error {
	srcInfo, err := os.Stat(source)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("source %s: %w", source, err)
	}
	if !srcInfo.IsDir() {
		return fmt.Errorf("source %s is not a directory", source)
	}

	if isSymlink(dest) {
		_ = os.Remove(dest)
		return copyDirRecursiveExcept(source, dest, skip)
	}

	if _, err := os.Stat(dest); os.IsNotExist(err) {
		return copyDirRecursiveExcept(source, dest, skip)
	}

	srcFiles := make(map[string]os.FileInfo)
	if err := filepath.Walk(source, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return nil
		}
		rel, _ := filepath.Rel(source, path)
		if rel == "." {
			return nil
		}
		if skip != nil && skip(filepath.ToSlash(rel)) {
			return nil
		}
		srcFiles[rel] = info
		return nil
	}); err != nil {
		return fmt.Errorf("walking source %s: %w", source, err)
	}

	_ = filepath.Walk(dest, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return nil
		}
		rel, _ := filepath.Rel(dest, path)
		if rel == "." {
			return nil
		}
		if skip != nil && skip(filepath.ToSlash(rel)) {
			return nil
		}
		if _, exists := srcFiles[rel]; !exists {
			if info.IsDir() {
				_ = os.RemoveAll(path)
				return filepath.SkipDir
			}
			_ = os.Remove(path)
		}
		return nil
	})

	for rel, srcInfo := range srcFiles {
		srcPath := filepath.Join(source, rel)
		destPath := filepath.Join(dest, rel)

		if srcInfo.IsDir() {
			if err := os.MkdirAll(destPath, srcInfo.Mode()|0o755); err != nil {
				return fmt.Errorf("mkdir %s: %w", destPath, err)
			}
			continue
		}

		destInfo, err := os.Stat(destPath)
		if err == nil && destInfo.Size() == srcInfo.Size() && !destInfo.ModTime().Before(srcInfo.ModTime()) {
			continue
		}

		if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
			return fmt.Errorf("mkdir parent %s: %w", destPath, err)
		}
		if err := copyFile(srcPath, destPath, srcInfo.Mode()); err != nil {
			return err
		}
	}

	return nil
}

func copyDirRecursive(source, dest string) error {
	return copyDirRecursiveExcept(source, dest, nil)
}

// copyDirRecursiveExcept copies a tree, honouring the caller's skip rule.
//
// The rule has to be applied HERE and not only in the mirroring walks: a missing
// destination takes this path instead, so an exclusion enforced only by the mirror
// was silently ignored on the very first copy — which is the common case.
func copyDirRecursiveExcept(source, dest string, skip func(rel string) bool) error {
	return filepath.Walk(source, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return nil
		}
		rel, _ := filepath.Rel(source, path)
		if rel != "." && skip != nil && skip(filepath.ToSlash(rel)) {
			if info.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		target := filepath.Join(dest, rel)

		if info.IsDir() {
			return os.MkdirAll(target, info.Mode()|0o755)
		}
		return copyFile(path, target, info.Mode())
	})
}

func copyFile(src, dst string, perm os.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return fmt.Errorf("open %s: %w", src, err)
	}
	defer func() { _ = in.Close() }()

	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, perm)
	if err != nil {
		return fmt.Errorf("create %s: %w", dst, err)
	}
	defer func() { _ = out.Close() }()

	if _, err := io.Copy(out, in); err != nil {
		return fmt.Errorf("copy %s → %s: %w", src, dst, err)
	}
	return out.Close()
}

func removeIfSymlink(path string) {
	if info, err := os.Lstat(path); err == nil && info.Mode()&os.ModeSymlink != 0 {
		_ = os.Remove(path)
	}
}

func isSymlink(path string) bool {
	info, err := os.Lstat(path)
	if err != nil {
		return false
	}
	return info.Mode()&os.ModeSymlink != 0
}
