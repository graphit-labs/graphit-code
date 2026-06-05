package main

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const modelCacheSubdir = "models/coderankembed"

// deduplicateModels moves extracted model files from the runtime dir to a
// shared cache dir and replaces them with symlinks. This avoids storing
// a second ~132 MB copy of model.onnx per version upgrade.
//
// On Windows without Developer Mode / admin privileges, os.Symlink may
// fail; in that case the original file is left in place and everything
// still works — just without the disk saving.
func deduplicateModels(runtimeDir string) {
	modelsDir := filepath.Join(runtimeDir, "models")
	if _, err := os.Stat(modelsDir); err != nil {
		return
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return
	}
	cacheDir := filepath.Join(home, brand.DotDir(), modelCacheSubdir)

	files := []string{"model.onnx", "tokenizer.json"}
	for _, name := range files {
		deduplicateFile(filepath.Join(modelsDir, name), filepath.Join(cacheDir, name))
	}
}

func deduplicateFile(runtimePath, cachePath string) {

	if fi, err := os.Lstat(runtimePath); err != nil || fi.Mode()&os.ModeSymlink != 0 {
		return
	}

	runtimeInfo, err := os.Stat(runtimePath)
	if err != nil || runtimeInfo.Size() == 0 {
		return
	}

	if err := os.MkdirAll(filepath.Dir(cachePath), 0o755); err != nil {
		return
	}

	cacheInfo, cacheErr := os.Stat(cachePath)
	if cacheErr != nil || cacheInfo.Size() < runtimeInfo.Size() {

		if err := moveFile(runtimePath, cachePath); err != nil {
			return
		}
	} else {

		_ = os.Remove(runtimePath)
	}

	if err := os.Symlink(cachePath, runtimePath); err != nil {

		if _, statErr := os.Stat(runtimePath); os.IsNotExist(statErr) {
			_ = copyFile(cachePath, runtimePath)
		}
	}
}


func moveFile(src, dst string) error {
	if err := os.Rename(src, dst); err == nil {
		return nil
	}

	if err := copyFile(src, dst); err != nil {
		return err
	}
	return os.Remove(src)
}

func copyFile(src, dst string) error {
	data, err := os.ReadFile(src)
	if err != nil {
		return fmt.Errorf("read %s: %w", src, err)
	}
	return os.WriteFile(dst, data, 0o644)
}
