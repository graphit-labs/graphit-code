package hub

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

func HashPath(path string) (string, error) {
	info, err := os.Stat(path)
	if err != nil {
		return "", fmt.Errorf("stat %q: %w", path, err)
	}

	if !info.IsDir() {
		return HashFile(path)
	}

	return hashDirectory(path)
}

func HashDirectory(dir string) (string, error) {
	return HashPath(dir)
}

func hashDirectory(dir string) (string, error) {
	var entries []string

	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}

		if info.Mode()&os.ModeSymlink != 0 {
			return nil
		}
		rel, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}

		entries = append(entries, filepath.ToSlash(rel))
		return nil
	})
	if err != nil {
		return "", fmt.Errorf("walking directory %q: %w", dir, err)
	}

	if len(entries) == 0 {
		return "", fmt.Errorf("directory %q contains no files", dir)
	}

	sort.Strings(entries)

	outer := sha256.New()
	for _, rel := range entries {
		absPath := filepath.Join(dir, filepath.FromSlash(rel))

		h := sha256.New()
		h.Write([]byte(rel))
		h.Write([]byte{0})

		f, err := os.Open(absPath)
		if err != nil {
			return "", fmt.Errorf("opening %q: %w", rel, err)
		}
		if _, err := io.Copy(h, f); err != nil {
			_ = f.Close()
			return "", fmt.Errorf("reading %q: %w", rel, err)
		}
		_ = f.Close()

		outer.Write(h.Sum(nil))
	}

	return hex.EncodeToString(outer.Sum(nil)), nil
}

func HashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() { _ = f.Close() }()

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func TruncateHash(hash string, n int) string {
	if len(hash) <= n {
		return hash
	}
	return hash[:n]
}

func VerifyHash(path, expectedHash string) (bool, error) {
	if expectedHash == "" {
		return true, nil
	}
	actual, err := HashPath(path)
	if err != nil {
		return false, err
	}
	return strings.EqualFold(actual, expectedHash), nil
}
