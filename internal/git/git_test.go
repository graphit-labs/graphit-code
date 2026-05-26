package git

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBlockManager(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-git-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	filePath := filepath.Join(tempDir, "test.sh")

	// 1. Inject block to non-existent file
	err = InjectBlock(filePath, "echo 'hello'", "M1", "#!/bin/sh")
	if err != nil {
		t.Fatalf("InjectBlock failed: %v", err)
	}

	data, err := os.ReadFile(filePath)
	if err != nil {
		t.Fatalf("failed to read file: %v", err)
	}

	expected := "#!/bin/sh\n\n# --- M1 ---\necho 'hello'\n# --- END M1 ---\n"
	if string(data) != expected {
		t.Errorf("expected:\n%q\ngot:\n%q", expected, string(data))
	}

	// 2. Inject existing block with new content
	err = InjectBlock(filePath, "echo 'hello2'", "M1", "#!/bin/sh")
	if err != nil {
		t.Fatalf("InjectBlock failed on update: %v", err)
	}

	data, _ = os.ReadFile(filePath)
	expectedUpdate := "#!/bin/sh\n\n# --- M1 ---\necho 'hello2'\n# --- END M1 ---\n"
	if string(data) != expectedUpdate {
		t.Errorf("expected:\n%q\ngot:\n%q", expectedUpdate, string(data))
	}

	// 3. Remove block
	removed, err := RemoveBlock(filePath, "M1", false)
	if err != nil {
		t.Fatalf("RemoveBlock failed: %v", err)
	}
	if !removed {
		t.Error("expected block to be removed")
	}

	data, _ = os.ReadFile(filePath)
	if strings.TrimSpace(string(data)) != "#!/bin/sh" {
		t.Errorf("expected file to contain shebang only, got %q", string(data))
	}

	// 4. Remove block with deleteIfEmpty
	_ = os.WriteFile(filePath, []byte("#!/bin/sh\n# --- M1 ---\necho 'hello'\n# --- END M1 ---\n"), 0755)
	removed, err = RemoveBlock(filePath, "M1", true)
	if err != nil {
		t.Fatalf("RemoveBlock failed: %v", err)
	}
	if !removed {
		t.Error("expected block to be removed")
	}
	if _, err := os.Stat(filePath); !os.IsNotExist(err) {
		t.Error("expected file to be deleted because it only had shebang left")
	}
}

func TestHTMLStyledBlock(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-git-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	filePath := filepath.Join(tempDir, "test.html")

	err = InjectBlockStyled(filePath, "<div>hello</div>", "HTML_M", "", HTMLBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	expected := "<!-- HTML_M -->\n<div>hello</div>\n<!-- END HTML_M -->\n"
	if string(data) != expected {
		t.Errorf("expected:\n%q\ngot:\n%q", expected, string(data))
	}

	removed, err := RemoveBlockStyled(filePath, "HTML_M", false, HTMLBlockStyle)
	if err != nil || !removed {
		t.Errorf("failed to remove styled block: %v, %t", err, removed)
	}
}

func TestGitignoreHelpers(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-git-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	filePath := filepath.Join(tempDir, ".gitignore")
	err = InjectGitignore(filePath, "*.log")
	if err != nil {
		t.Fatalf("InjectGitignore failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	if !strings.Contains(string(data), "*.log") {
		t.Errorf("expected *.log in .gitignore, got %s", data)
	}

	removed, err := RemoveGitignore(filePath)
	if err != nil || !removed {
		t.Errorf("RemoveGitignore failed: %v, %t", err, removed)
	}
}

func TestGitCLIBackend(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-git-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tempDir)

	// Ensure Git is installed in PATH to run tests
	g := Default()
	if g == nil {
		t.Fatal("expected non-nil default Git instance")
	}

	// 1. RunGlobalOutput
	versionStr, err := g.RunGlobalOutput("version")
	if err != nil {
		t.Fatalf("RunGlobalOutput failed: %v", err)
	}
	if !strings.Contains(versionStr, "git version") {
		t.Errorf("expected 'git version', got %q", versionStr)
	}

	// 2. Initialize a repository inside tempDir
	err = g.Run(tempDir, "init")
	if err != nil {
		t.Fatalf("git init failed: %v", err)
	}

	// 3. Configure local user for test commits
	_ = g.Run(tempDir, "config", "local", "user.name", "Test User")
	_ = g.Run(tempDir, "config", "local", "user.email", "test@example.com")

	// 4. Create and commit a file
	filePath := filepath.Join(tempDir, "file.txt")
	_ = os.WriteFile(filePath, []byte("git test content"), 0644)

	err = g.Run(tempDir, "add", "file.txt")
	if err != nil {
		t.Fatalf("git add failed: %v", err)
	}

	err = g.Run(tempDir, "commit", "-m", "initial commit")
	if err != nil {
		t.Fatalf("git commit failed: %v", err)
	}

	// 5. RunOutput
	logOut, err := g.RunOutput(tempDir, "log", "-n", "1", "--oneline")
	if err != nil {
		t.Fatalf("git log failed: %v", err)
	}
	if !strings.Contains(logOut, "initial commit") {
		t.Errorf("log output missing message: %s", logOut)
	}

	// 6. RunSilent
	res := g.RunSilent(tempDir, "status")
	if !strings.Contains(res, "nothing to commit") {
		t.Errorf("RunSilent status unexpected: %q", res)
	}

	// 7. RunWithStdin
	patchStr, err := g.RunWithStdin(tempDir, []byte("line of content"), "hash-object", "-w", "--stdin")
	if err != nil {
		t.Fatalf("RunWithStdin failed: %v", err)
	}
	if len(patchStr) == 0 {
		t.Error("expected non-empty blob hash")
	}

	// 8. RunOutputWithEnv
	outEnv, err := g.RunOutputWithEnv(tempDir, map[string]string{"GIT_AUTHOR_NAME": "Override Author", "GIT_AUTHOR_EMAIL": "override@example.com"}, "var", "GIT_AUTHOR_IDENT")
	if err != nil {
		t.Fatalf("RunOutputWithEnv failed: %v", err)
	}
	if !strings.Contains(outEnv, "Override Author") {
		t.Errorf("expected 'Override Author' in GIT_AUTHOR_IDENT, got %q", outEnv)
	}

	// 9. RunWithEnv
	err = g.RunWithEnv(tempDir, map[string]string{"GIT_AUTHOR_NAME": "Override Env", "GIT_AUTHOR_EMAIL": "override@example.com"}, "commit", "--allow-empty", "-m", "env commit")
	if err != nil {
		t.Fatalf("RunWithEnv failed: %v", err)
	}

	// 10. RunGlobal
	err = g.RunGlobal("version")
	if err != nil {
		t.Errorf("RunGlobal failed: %v", err)
	}
}
