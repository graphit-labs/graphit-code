package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDeduplicateFile_MovesToCacheAndSymlinks(t *testing.T) {
	runtimeDir := t.TempDir()
	cacheDir := t.TempDir()

	runtimePath := filepath.Join(runtimeDir, "model.onnx")
	cachePath := filepath.Join(cacheDir, "model.onnx")

	if err := os.WriteFile(runtimePath, make([]byte, 1000), 0o644); err != nil {
		t.Fatal(err)
	}

	deduplicateFile(runtimePath, cachePath)


	fi, err := os.Lstat(runtimePath)
	if err != nil {
		t.Fatalf("runtime file missing after dedup: %v", err)
	}
	if fi.Mode()&os.ModeSymlink == 0 {
		t.Error("runtime path should be a symlink")
	}


	target, err := os.Readlink(runtimePath)
	if err != nil {
		t.Fatalf("readlink failed: %v", err)
	}
	if target != cachePath {
		t.Errorf("symlink target = %q; want %q", target, cachePath)
	}


	cacheInfo, err := os.Stat(cachePath)
	if err != nil {
		t.Fatalf("cache file missing: %v", err)
	}
	if cacheInfo.Size() != 1000 {
		t.Errorf("cache size = %d; want 1000", cacheInfo.Size())
	}
}

func TestDeduplicateFile_CacheAlreadyValid(t *testing.T) {
	runtimeDir := t.TempDir()
	cacheDir := t.TempDir()

	runtimePath := filepath.Join(runtimeDir, "model.onnx")
	cachePath := filepath.Join(cacheDir, "model.onnx")


	if err := os.WriteFile(cachePath, make([]byte, 2000), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(runtimePath, make([]byte, 1000), 0o644); err != nil {
		t.Fatal(err)
	}

	deduplicateFile(runtimePath, cachePath)

	fi, err := os.Lstat(runtimePath)
	if err != nil {
		t.Fatalf("runtime file missing: %v", err)
	}
	if fi.Mode()&os.ModeSymlink == 0 {
		t.Error("runtime path should be a symlink")
	}


	cacheInfo, err := os.Stat(cachePath)
	if err != nil {
		t.Fatal(err)
	}
	if cacheInfo.Size() != 2000 {
		t.Errorf("cache should retain original: size = %d; want 2000", cacheInfo.Size())
	}
}

func TestDeduplicateFile_AlreadySymlink(t *testing.T) {
	runtimeDir := t.TempDir()
	cacheDir := t.TempDir()

	runtimePath := filepath.Join(runtimeDir, "model.onnx")
	cachePath := filepath.Join(cacheDir, "model.onnx")

	if err := os.WriteFile(cachePath, make([]byte, 500), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(cachePath, runtimePath); err != nil {
		t.Skip("symlinks not supported")
	}


	deduplicateFile(runtimePath, cachePath)

	fi, err := os.Lstat(runtimePath)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode()&os.ModeSymlink == 0 {
		t.Error("should still be a symlink")
	}
}

func TestDeduplicateFile_MissingRuntimeFile(t *testing.T) {
	runtimeDir := t.TempDir()
	cacheDir := t.TempDir()


	deduplicateFile(
		filepath.Join(runtimeDir, "missing.onnx"),
		filepath.Join(cacheDir, "missing.onnx"),
	)
}

func TestDeduplicateFile_EmptyFile(t *testing.T) {
	runtimeDir := t.TempDir()
	cacheDir := t.TempDir()

	runtimePath := filepath.Join(runtimeDir, "empty.onnx")
	cachePath := filepath.Join(cacheDir, "empty.onnx")

	if err := os.WriteFile(runtimePath, nil, 0o644); err != nil {
		t.Fatal(err)
	}


	deduplicateFile(runtimePath, cachePath)

	fi, err := os.Lstat(runtimePath)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		t.Error("empty file should not be symlinked")
	}
}

func TestDeduplicateModels_NoModelsDir(t *testing.T) {

	deduplicateModels(t.TempDir())
}

func TestDeduplicateModels_WithModelsDir(t *testing.T) {
	runtimeDir := t.TempDir()
	modelsDir := filepath.Join(runtimeDir, "models")
	if err := os.MkdirAll(modelsDir, 0o755); err != nil {
		t.Fatal(err)
	}

	modelPath := filepath.Join(modelsDir, "model.onnx")
	tokenizerPath := filepath.Join(modelsDir, "tokenizer.json")

	if err := os.WriteFile(modelPath, make([]byte, 500), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(tokenizerPath, make([]byte, 100), 0o644); err != nil {
		t.Fatal(err)
	}

	deduplicateModels(runtimeDir)


	for _, path := range []string{modelPath, tokenizerPath} {
		fi, err := os.Lstat(path)
		if err != nil {
			t.Errorf("file %s missing after dedup: %v", filepath.Base(path), err)
			continue
		}
		if fi.Mode()&os.ModeSymlink == 0 {
			t.Errorf("%s should be a symlink", filepath.Base(path))
		}
	}
}

func TestMoveFile_SameDevice(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src.bin")
	dst := filepath.Join(dir, "dst.bin")

	content := []byte("move me")
	if err := os.WriteFile(src, content, 0o644); err != nil {
		t.Fatal(err)
	}

	if err := moveFile(src, dst); err != nil {
		t.Fatalf("moveFile failed: %v", err)
	}

	if _, err := os.Stat(src); !os.IsNotExist(err) {
		t.Error("source should be removed after move")
	}

	data, err := os.ReadFile(dst)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "move me" {
		t.Errorf("content = %q; want %q", data, "move me")
	}
}

func TestCopyFile(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src.bin")
	dst := filepath.Join(dir, "dst.bin")

	content := []byte("copy me")
	if err := os.WriteFile(src, content, 0o644); err != nil {
		t.Fatal(err)
	}

	if err := copyFile(src, dst); err != nil {
		t.Fatalf("copyFile failed: %v", err)
	}


	if _, err := os.Stat(src); err != nil {
		t.Error("source should still exist after copy")
	}

	data, err := os.ReadFile(dst)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "copy me" {
		t.Errorf("content = %q; want %q", data, "copy me")
	}
}
