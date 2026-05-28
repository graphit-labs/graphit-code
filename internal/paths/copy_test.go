package paths

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestSafeCopyDir(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "sub"), 0o755)
	_ = os.WriteFile(filepath.Join(src, "a.txt"), []byte("hello"), 0o644)
	_ = os.WriteFile(filepath.Join(src, "sub", "b.txt"), []byte("world"), 0o644)

	dst := filepath.Join(tmp, "dst")
	if err := SafeCopyDir(src, dst); err != nil {
		t.Fatalf("SafeCopyDir failed: %v", err)
	}

	data, err := os.ReadFile(filepath.Join(dst, "a.txt"))
	if err != nil || string(data) != "hello" {
		t.Errorf("expected 'hello', got %q (err=%v)", string(data), err)
	}

	data, err = os.ReadFile(filepath.Join(dst, "sub", "b.txt"))
	if err != nil || string(data) != "world" {
		t.Errorf("expected 'world', got %q (err=%v)", string(data), err)
	}

	info, err := os.Lstat(dst)
	if err != nil {
		t.Fatalf("stat dst: %v", err)
	}
	if info.Mode()&os.ModeSymlink != 0 {
		t.Error("dst should be a real directory, not a symlink")
	}
}

func TestSafeCopyDir_OverwritesExistingDir(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)
	_ = os.WriteFile(filepath.Join(src, "new.txt"), []byte("new"), 0o644)

	dst := filepath.Join(tmp, "dst")
	_ = os.MkdirAll(dst, 0o755)
	_ = os.WriteFile(filepath.Join(dst, "old.txt"), []byte("old"), 0o644)

	if err := SafeCopyDir(src, dst); err != nil {
		t.Fatalf("SafeCopyDir failed: %v", err)
	}

	if _, err := os.Stat(filepath.Join(dst, "old.txt")); !os.IsNotExist(err) {
		t.Error("old.txt should not exist after SafeCopyDir")
	}

	data, _ := os.ReadFile(filepath.Join(dst, "new.txt"))
	if string(data) != "new" {
		t.Errorf("expected 'new', got %q", string(data))
	}
}

func TestSafeCopyDir_ReplacesSymlink(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)
	_ = os.WriteFile(filepath.Join(src, "file.txt"), []byte("data"), 0o644)

	target := filepath.Join(tmp, "target")
	_ = os.MkdirAll(target, 0o755)

	dst := filepath.Join(tmp, "dst")
	_ = os.Symlink(target, dst)

	if err := SafeCopyDir(src, dst); err != nil {
		t.Fatalf("SafeCopyDir failed: %v", err)
	}

	info, _ := os.Lstat(dst)
	if info.Mode()&os.ModeSymlink != 0 {
		t.Error("dst should be a real dir after copy, not a symlink")
	}

	data, _ := os.ReadFile(filepath.Join(dst, "file.txt"))
	if string(data) != "data" {
		t.Errorf("expected 'data', got %q", string(data))
	}
}

func TestSafeCopyDir_EmptyDir(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)

	dst := filepath.Join(tmp, "dst")
	if err := SafeCopyDir(src, dst); err != nil {
		t.Fatalf("SafeCopyDir failed: %v", err)
	}

	info, err := os.Stat(dst)
	if err != nil {
		t.Fatalf("stat dst: %v", err)
	}
	if !info.IsDir() {
		t.Error("dst should be a directory")
	}
}

func TestSyncCopyDir(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "sub"), 0o755)
	_ = os.WriteFile(filepath.Join(src, "a.txt"), []byte("hello"), 0o644)
	_ = os.WriteFile(filepath.Join(src, "sub", "b.txt"), []byte("world"), 0o644)

	dst := filepath.Join(tmp, "dst")
	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("SyncCopyDir initial failed: %v", err)
	}

	data, _ := os.ReadFile(filepath.Join(dst, "a.txt"))
	if string(data) != "hello" {
		t.Errorf("expected 'hello', got %q", string(data))
	}

	time.Sleep(10 * time.Millisecond)
	_ = os.WriteFile(filepath.Join(src, "a.txt"), []byte("updated"), 0o644)

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("SyncCopyDir update failed: %v", err)
	}

	data, _ = os.ReadFile(filepath.Join(dst, "a.txt"))
	if string(data) != "updated" {
		t.Errorf("expected 'updated', got %q", string(data))
	}
}

func TestSyncCopyDir_RemovesObsolete(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)
	_ = os.WriteFile(filepath.Join(src, "keep.txt"), []byte("keep"), 0o644)
	_ = os.WriteFile(filepath.Join(src, "remove.txt"), []byte("remove"), 0o644)

	dst := filepath.Join(tmp, "dst")
	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("initial sync failed: %v", err)
	}

	_ = os.Remove(filepath.Join(src, "remove.txt"))

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("second sync failed: %v", err)
	}

	if _, err := os.Stat(filepath.Join(dst, "remove.txt")); !os.IsNotExist(err) {
		t.Error("remove.txt should have been deleted")
	}
	if _, err := os.Stat(filepath.Join(dst, "keep.txt")); err != nil {
		t.Error("keep.txt should still exist")
	}
}

func TestSyncCopyDir_ReplacesSymlink(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)
	_ = os.WriteFile(filepath.Join(src, "file.txt"), []byte("data"), 0o644)

	target := filepath.Join(tmp, "target")
	_ = os.MkdirAll(target, 0o755)

	dst := filepath.Join(tmp, "dst")
	_ = os.Symlink(target, dst)

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("SyncCopyDir failed: %v", err)
	}

	info, _ := os.Lstat(dst)
	if info.Mode()&os.ModeSymlink != 0 {
		t.Error("dst should be a real dir after sync, not a symlink")
	}

	data, _ := os.ReadFile(filepath.Join(dst, "file.txt"))
	if string(data) != "data" {
		t.Errorf("expected 'data', got %q", string(data))
	}
}

func TestSyncCopyDir_NonexistentSource(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "nonexistent")
	dst := filepath.Join(tmp, "dst")

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("SyncCopyDir should return nil for nonexistent source, got: %v", err)
	}
}
