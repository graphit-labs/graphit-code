package paths

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestGetPaths(t *testing.T) {
	// Isolate HOME
	origHome := os.Getenv("HOME")
	tempDir, err := os.MkdirTemp("", "paths-test-home")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}
	defer os.RemoveAll(tempDir)
	os.Setenv("HOME", tempDir)
	defer os.Setenv("HOME", origHome)

	// Test GetPaths
	p := GetPaths("test-ide", false)
	if p.IDE != "test-ide" {
		t.Errorf("expected IDE to be test-ide, got %q", p.IDE)
	}

	pGlobal := GetPaths("test-ide", true)
	if pGlobal.TargetDir != pGlobal.FrameworksDir {
		t.Errorf("expected global targetDir to match frameworksDir")
	}

	// Test GetPathsForProject
	pProj := GetPathsForProject("test-ide", tempDir)
	if pProj.ActiveProjectDir != tempDir {
		t.Errorf("expected projectDir to match activeProjectDir, got %q", pProj.ActiveProjectDir)
	}

	// Test empty ProjectDir fallback
	pProjEmpty := GetPathsForProject("test-ide", "")
	if pProjEmpty.ActiveProjectDir == "" {
		t.Error("expected active project dir to be resolved even with empty input")
	}
}

func TestResolveGitDir(t *testing.T) {
	tempProj, err := os.MkdirTemp("", "paths-gitdir-test")
	if err != nil {
		t.Fatalf("failed to create temp project: %v", err)
	}
	defer os.RemoveAll(tempProj)

	// Case 1: .git does not exist
	gitDir1 := resolveGitDir(tempProj)
	if gitDir1 != filepath.Join(tempProj, ".git") {
		t.Errorf("expected default .git path, got %q", gitDir1)
	}

	// Case 2: .git is a directory
	dotGitDir := filepath.Join(tempProj, ".git")
	err = os.MkdirAll(dotGitDir, 0755)
	if err != nil {
		t.Fatalf("failed to create .git dir: %v", err)
	}
	gitDir2 := resolveGitDir(tempProj)
	if gitDir2 != dotGitDir {
		t.Errorf("expected directory %q, got %q", dotGitDir, gitDir2)
	}

	// Clean up .git directory
	os.RemoveAll(dotGitDir)

	// Case 3: .git is a file but empty/invalid
	err = os.WriteFile(dotGitDir, []byte("invalid content"), 0644)
	if err != nil {
		t.Fatalf("failed to write invalid .git file: %v", err)
	}
	gitDir3 := resolveGitDir(tempProj)
	if gitDir3 != dotGitDir {
		t.Errorf("expected fallback to %q, got %q", dotGitDir, gitDir3)
	}

	// Case 4: .git is a file pointing to absolute path
	absPath := filepath.Join(tempProj, "abs-git-dir")
	err = os.WriteFile(dotGitDir, []byte("gitdir: "+absPath), 0644)
	if err != nil {
		t.Fatalf("failed to write abs .git file: %v", err)
	}
	gitDir4 := resolveGitDir(tempProj)
	if gitDir4 != absPath {
		t.Errorf("expected absolute path %q, got %q", absPath, gitDir4)
	}

	// Case 5: .git is a file pointing to relative path
	relPath := "rel-git-dir"
	err = os.WriteFile(dotGitDir, []byte("gitdir: "+relPath), 0644)
	if err != nil {
		t.Fatalf("failed to write rel .git file: %v", err)
	}
	gitDir5 := resolveGitDir(tempProj)
	expectedRel := filepath.Join(tempProj, relPath)
	if gitDir5 != expectedRel {
		t.Errorf("expected absolute resolved path %q, got %q", expectedRel, gitDir5)
	}

	// Case 6: .git is an unreadable file
	err = os.WriteFile(dotGitDir, []byte("gitdir: unreadable"), 0000)
	if err != nil {
		t.Fatalf("failed to write unreadable .git file: %v", err)
	}
	gitDir6 := resolveGitDir(tempProj)
	// If the file was unreadable, it returns dotGitDir
	// If the file was readable (e.g. running as root), it returns filepath.Join(tempProj, "unreadable")
	if gitDir6 != dotGitDir && gitDir6 != filepath.Join(tempProj, "unreadable") {
		t.Errorf("unexpected gitDir: %q", gitDir6)
	}
}

func TestSafeSymlink(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "paths-symlink-test")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	srcFile := filepath.Join(tempDir, "src.txt")
	err = os.WriteFile(srcFile, []byte("hello"), 0644)
	if err != nil {
		t.Fatalf("failed to write src: %v", err)
	}

	linkFile := filepath.Join(tempDir, "link.txt")

	// 1. Create fresh symlink
	err = SafeSymlink(srcFile, linkFile)
	if err != nil {
		t.Fatalf("SafeSymlink failed: %v", err)
	}

	// Verify link contents
	data, err := os.ReadFile(linkFile)
	if err != nil || string(data) != "hello" {
		t.Errorf("expected hello from link, got %q (err=%v)", string(data), err)
	}

	// 2. Overwrite existing symlink
	err = SafeSymlink(srcFile, linkFile)
	if err != nil {
		t.Fatalf("SafeSymlink overwrite failed: %v", err)
	}

	// 3. Overwrite when target exists as a regular file
	err = os.Remove(linkFile)
	if err != nil {
		t.Fatalf("failed to remove link: %v", err)
	}
	err = os.WriteFile(linkFile, []byte("existing file"), 0644)
	if err != nil {
		t.Fatalf("failed to write conflicting file: %v", err)
	}

	err = SafeSymlink(srcFile, linkFile)
	if err != nil {
		t.Fatalf("SafeSymlink overwrite regular file failed: %v", err)
	}

	// 4. Overwrite when target exists as a directory
	err = os.Remove(linkFile)
	if err != nil {
		t.Fatalf("failed to remove link: %v", err)
	}
	err = os.Mkdir(linkFile, 0755)
	if err != nil {
		t.Fatalf("failed to create conflicting dir: %v", err)
	}

	err = SafeSymlink(srcFile, linkFile)
	if err != nil {
		t.Fatalf("SafeSymlink overwrite directory failed: %v", err)
	}

	// 5. Test symlink to a directory
	srcDir := filepath.Join(tempDir, "srcdir")
	err = os.Mkdir(srcDir, 0755)
	if err != nil {
		t.Fatalf("failed to create src dir: %v", err)
	}

	linkDir := filepath.Join(tempDir, "linkdir")
	err = SafeSymlink(srcDir, linkDir)
	if err != nil {
		t.Fatalf("SafeSymlink for directory failed: %v", err)
	}

	// 6. Test Windows fallback junction function (just call directly for line coverage)
	errFallback := windowsFallbackJunction("source", "linkPath", false, os.ErrExist)
	if errFallback == nil {
		t.Error("expected error from fallback, got nil")
	}

	// 7. Test SafeSymlink failure path
	errFail := SafeSymlink("/nonexistent/source", "/nonexistent/dir/link")
	if errFail == nil {
		t.Error("expected SafeSymlink to fail for nonexistent paths, but it succeeded")
	}
}

func TestBuildPathsHooks(t *testing.T) {
	tempProj, err := os.MkdirTemp("", "paths-hooks-test")
	if err != nil {
		t.Fatalf("failed to create temp project: %v", err)
	}
	defer os.RemoveAll(tempProj)

	// Initialize real git repo so git config queries will execute successfully
	cmdInit := exec.Command("git", "init")
	cmdInit.Dir = tempProj
	err = cmdInit.Run()
	if err != nil {
		t.Fatalf("git init failed: %v", err)
	}

	// 1. Without core.hooksPath set (should default to resolveGitDir/hooks)
	p := buildPaths("", tempProj)
	expectedHooks := filepath.Join(tempProj, ".git", "hooks")
	if p.RepoHooksDir != expectedHooks {
		t.Errorf("expected default hooks path to be %q, got %q", expectedHooks, p.RepoHooksDir)
	}

	// 2. With core.hooksPath set as absolute path
	cmdConfigAbs := exec.Command("git", "config", "core.hooksPath", "/abs/custom/hooks")
	cmdConfigAbs.Dir = tempProj
	_ = cmdConfigAbs.Run()

	pAbs := buildPaths("", tempProj)
	if pAbs.RepoHooksDir != "/abs/custom/hooks" {
		t.Errorf("expected absolute hooks path to be '/abs/custom/hooks', got %q", pAbs.RepoHooksDir)
	}

	// 3. With core.hooksPath set as relative path
	cmdConfigRel := exec.Command("git", "config", "core.hooksPath", "rel/custom/hooks")
	cmdConfigRel.Dir = tempProj
	_ = cmdConfigRel.Run()

	pRel := buildPaths("", tempProj)
	expectedRelHooks := filepath.Join(tempProj, "rel/custom/hooks")
	if pRel.RepoHooksDir != expectedRelHooks {
		t.Errorf("expected relative hooks path to be %q, got %q", expectedRelHooks, pRel.RepoHooksDir)
	}
}
