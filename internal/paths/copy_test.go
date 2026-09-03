package paths

import (
	"os"
	"path/filepath"
	"strings"
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

func TestSafeCopyDirSourceNotExist(t *testing.T) {
	tmp := t.TempDir()
	err := SafeCopyDir(filepath.Join(tmp, "nonexistent"), filepath.Join(tmp, "dst"))
	if err == nil {
		t.Error("expected error for nonexistent source")
	}
}

func TestSafeCopyDirSourceIsFile(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "file.txt")
	_ = os.WriteFile(src, []byte("data"), 0644)

	err := SafeCopyDir(src, filepath.Join(tmp, "dst"))
	if err == nil || !strings.Contains(err.Error(), "not a directory") {
		t.Errorf("expected 'not a directory' error, got %v", err)
	}
}

func TestSafeCopyDirRemoveAllError(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0755)

	dst := filepath.Join(tmp, "readonly", "dst")
	_ = os.MkdirAll(dst, 0755)
	_ = os.WriteFile(filepath.Join(dst, "file.txt"), []byte("data"), 0644)
	_ = os.Chmod(filepath.Join(tmp, "readonly"), 0555)
	defer func() { _ = os.Chmod(filepath.Join(tmp, "readonly"), 0755) }()

	err := SafeCopyDir(src, dst)
	if err == nil {
		t.Error("expected error when RemoveAll fails")
	}
}

func TestSyncCopyDirSourceIsFile(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "file.txt")
	_ = os.WriteFile(src, []byte("data"), 0644)

	err := SyncCopyDir(src, filepath.Join(tmp, "dst"))
	if err == nil || !strings.Contains(err.Error(), "not a directory") {
		t.Errorf("expected 'not a directory' error, got %v", err)
	}
}

func TestSyncCopyDirSourceStatError(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "noperm")
	_ = os.MkdirAll(src, 0755)
	_ = os.WriteFile(filepath.Join(src, "file.txt"), []byte("data"), 0644)
	_ = os.Chmod(src, 0000)
	defer func() { _ = os.Chmod(src, 0755) }()

	err := SyncCopyDir(src, filepath.Join(tmp, "dst"))
	_ = err
}

func TestSyncCopyDirRemovesObsoleteDirs(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "keep"), 0755)
	_ = os.WriteFile(filepath.Join(src, "keep", "file.txt"), []byte("keep"), 0644)

	dst := filepath.Join(tmp, "dst")
	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("initial sync failed: %v", err)
	}

	_ = os.MkdirAll(filepath.Join(dst, "obsolete", "nested"), 0755)
	_ = os.WriteFile(filepath.Join(dst, "obsolete", "nested", "file.txt"), []byte("delete"), 0644)

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("second sync failed: %v", err)
	}

	if _, err := os.Stat(filepath.Join(dst, "obsolete")); !os.IsNotExist(err) {
		t.Error("obsolete directory should have been removed")
	}
}

func TestSyncCopyDirMkdirError(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "sub"), 0755)
	_ = os.WriteFile(filepath.Join(src, "sub", "file.txt"), []byte("data"), 0644)

	dst := filepath.Join(tmp, "dst")
	_ = os.MkdirAll(dst, 0755)
	_ = os.Chmod(dst, 0555)
	defer func() { _ = os.Chmod(dst, 0755) }()

	err := SyncCopyDir(src, dst)
	if err == nil {
		t.Error("expected error when mkdir fails in dest")
	}
}

func TestCopyFileSourceOpenError(t *testing.T) {
	tmp := t.TempDir()
	err := copyFile(filepath.Join(tmp, "nonexistent"), filepath.Join(tmp, "dst"), 0644)
	if err == nil {
		t.Error("expected error when source doesn't exist")
	}
}

func TestCopyFileDestCreateError(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src")
	_ = os.WriteFile(src, []byte("data"), 0644)

	err := copyFile(src, filepath.Join(tmp, "nonexistent", "dst"), 0644)
	if err == nil {
		t.Error("expected error when dest dir doesn't exist")
	}
}

func TestCopyDirRecursiveWalkError(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "sub"), 0755)
	_ = os.WriteFile(filepath.Join(src, "sub", "file.txt"), []byte("data"), 0644)
	_ = os.Chmod(filepath.Join(src, "sub"), 0000)
	defer func() { _ = os.Chmod(filepath.Join(src, "sub"), 0755) }()

	dst := filepath.Join(tmp, "dst")
	err := copyDirRecursive(src, dst)
	_ = err
}

func TestResolveGitDirWithFile(t *testing.T) {
	tmp := t.TempDir()

	gitDir := filepath.Join(tmp, "actual-git-dir")
	_ = os.MkdirAll(gitDir, 0755)

	dotGit := filepath.Join(tmp, ".git")
	_ = os.WriteFile(dotGit, []byte("gitdir: "+gitDir+"\n"), 0644)

	result := resolveGitDir(tmp)
	if result != gitDir {
		t.Errorf("expected %q, got %q", gitDir, result)
	}

	relGitDir := filepath.Join(tmp, "rel-test")
	_ = os.MkdirAll(relGitDir, 0755)
	relGitActual := filepath.Join(relGitDir, "actual-git")
	_ = os.MkdirAll(relGitActual, 0755)

	dotGitRel := filepath.Join(relGitDir, ".git")
	_ = os.WriteFile(dotGitRel, []byte("gitdir: actual-git\n"), 0644)

	result2 := resolveGitDir(relGitDir)
	expected := filepath.Clean(filepath.Join(relGitDir, "actual-git"))
	if result2 != expected {
		t.Errorf("expected %q, got %q", expected, result2)
	}
}

func TestResolveGitDirNotGitdir(t *testing.T) {
	tmp := t.TempDir()

	_ = os.WriteFile(filepath.Join(tmp, ".git"), []byte("some random content"), 0644)

	result := resolveGitDir(tmp)
	expected := filepath.Join(tmp, ".git")
	if result != expected {
		t.Errorf("expected %q, got %q", expected, result)
	}
}

func TestResolveGitDirReadFileError(t *testing.T) {
	tmp := t.TempDir()

	dotGit := filepath.Join(tmp, ".git")
	_ = os.WriteFile(dotGit, []byte("content"), 0644)
	_ = os.Chmod(dotGit, 0000)
	defer func() { _ = os.Chmod(dotGit, 0644) }()

	result := resolveGitDir(tmp)
	expected := filepath.Join(tmp, ".git")
	if result != expected {
		t.Errorf("expected %q, got %q", expected, result)
	}
}

func TestGetPathsFallbackToCwd(t *testing.T) {
	p := GetPaths("test-ide", false)
	if p == nil {
		t.Fatal("expected non-nil paths")
	}
	if p.IDE != "test-ide" {
		t.Errorf("expected IDE 'test-ide', got %q", p.IDE)
	}
}

func TestGetPathsGlobal(t *testing.T) {
	p := GetPaths("test-ide", true)
	if p == nil {
		t.Fatal("expected non-nil paths")
	}
	if p.TargetDir != p.FrameworksDir {
		t.Error("expected TargetDir to equal FrameworksDir in global mode")
	}
}

func TestGetPathsForProjectEmpty(t *testing.T) {
	p := GetPathsForProject("test-ide", "")
	if p == nil {
		t.Fatal("expected non-nil paths")
	}
}

func TestBuildPathsDefaultIDE(t *testing.T) {
	p := buildPaths("", "/tmp/project")
	if p.IDE != "antigravity" {
		t.Errorf("expected default IDE 'antigravity', got %q", p.IDE)
	}
}

func TestIsSymlink(t *testing.T) {
	tmp := t.TempDir()

	f := filepath.Join(tmp, "file.txt")
	_ = os.WriteFile(f, []byte("data"), 0644)
	if isSymlink(f) {
		t.Error("regular file should not be symlink")
	}

	link := filepath.Join(tmp, "link")
	_ = os.Symlink(f, link)
	if !isSymlink(link) {
		t.Error("symlink should be detected as symlink")
	}

	if isSymlink(filepath.Join(tmp, "nonexistent")) {
		t.Error("nonexistent path should not be symlink")
	}
}

func TestRemoveIfSymlink(t *testing.T) {
	tmp := t.TempDir()

	f := filepath.Join(tmp, "regular")
	_ = os.WriteFile(f, []byte("data"), 0644)
	removeIfSymlink(f)
	if _, err := os.Stat(f); err != nil {
		t.Error("regular file should not be removed")
	}

	target := filepath.Join(tmp, "target")
	_ = os.MkdirAll(target, 0755)
	link := filepath.Join(tmp, "link")
	_ = os.Symlink(target, link)
	removeIfSymlink(link)
	if _, err := os.Lstat(link); !os.IsNotExist(err) {
		t.Error("symlink should have been removed")
	}
}

func TestSyncCopyDir_SkipUnchangedFiles(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)
	_ = os.WriteFile(filepath.Join(src, "file.txt"), []byte("data"), 0o644)

	dst := filepath.Join(tmp, "dst")
	_ = os.MkdirAll(dst, 0o755)

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("first SyncCopyDir: %v", err)
	}

	destFile := filepath.Join(dst, "file.txt")
	now := time.Now().Add(1 * time.Hour)
	_ = os.Chtimes(destFile, now, now)

	if err := SyncCopyDir(src, dst); err != nil {
		t.Fatalf("second SyncCopyDir: %v", err)
	}

	data, _ := os.ReadFile(destFile)
	if string(data) != "data" {
		t.Errorf("expected 'data', got %q", string(data))
	}
}

func TestSyncCopyDir_MkdirParentError(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "sub"), 0o755)
	_ = os.WriteFile(filepath.Join(src, "sub", "file.txt"), []byte("data"), 0o644)

	dst := filepath.Join(tmp, "dst")
	_ = os.MkdirAll(dst, 0o755)
	_ = os.WriteFile(filepath.Join(dst, "sub"), []byte("blocking"), 0o644)

	err := SyncCopyDir(src, dst)
	if err == nil {
		t.Error("expected error when mkdir fails due to file blocking")
	}
	if err != nil && !strings.Contains(err.Error(), "mkdir") {
		t.Errorf("expected mkdir error, got: %v", err)
	}
}

func TestSyncCopyDir_CopyFileError(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(src, 0o755)

	unreadable := filepath.Join(src, "noperm.txt")
	_ = os.WriteFile(unreadable, []byte("secret"), 0o000)

	dst := filepath.Join(tmp, "dst")
	_ = os.MkdirAll(dst, 0o755)

	err := SyncCopyDir(src, dst)
	_ = err

	_ = os.Chmod(unreadable, 0o644)
}

func TestSyncCopyDir_WalkSourceError(t *testing.T) {
	tmp := t.TempDir()

	src := filepath.Join(tmp, "src")
	_ = os.MkdirAll(filepath.Join(src, "sub"), 0o755)
	_ = os.WriteFile(filepath.Join(src, "good.txt"), []byte("ok"), 0o644)

	dst := filepath.Join(tmp, "dst")
	_ = os.MkdirAll(dst, 0o755)

	brokenLink := filepath.Join(src, "broken")
	_ = os.Symlink("/nonexistent/target/never/exists", brokenLink)

	_ = SyncCopyDir(src, dst)
}

func TestGetPaths_GlobalFlag(t *testing.T) {
	origHome := os.Getenv("HOME")
	tempDir := t.TempDir()
	_ = os.Setenv("HOME", tempDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	p := GetPaths("test", true)
	if p.TargetDir != p.FrameworksDir {
		t.Errorf("expected global TargetDir to be FrameworksDir, got %q", p.TargetDir)
	}
	if p.LockFilePath == "" {
		t.Error("expected LockFilePath to be set for global mode")
	}
}
