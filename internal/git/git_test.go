package git

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestBlockManager(t *testing.T) {
	tempDir := t.TempDir()

	filePath := filepath.Join(tempDir, "test.sh")

	err := InjectBlock(filePath, "echo 'hello'", "M1", "#!/bin/sh")
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

	err = InjectBlock(filePath, "echo 'hello2'", "M1", "#!/bin/sh")
	if err != nil {
		t.Fatalf("InjectBlock failed on update: %v", err)
	}

	data, _ = os.ReadFile(filePath)
	expectedUpdate := "#!/bin/sh\n\n# --- M1 ---\necho 'hello2'\n# --- END M1 ---\n"
	if string(data) != expectedUpdate {
		t.Errorf("expected:\n%q\ngot:\n%q", expectedUpdate, string(data))
	}

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
	tempDir := t.TempDir()

	filePath := filepath.Join(tempDir, "test.html")

	err := InjectBlockStyled(filePath, "<div>hello</div>", "HTML_M", "", HTMLBlockStyle)
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
	tempDir := t.TempDir()

	filePath := filepath.Join(tempDir, ".gitignore")
	err := InjectGitignore(filePath, "*.log")
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
	tempDir := t.TempDir()

	g := Default()
	if g == nil {
		t.Fatal("expected non-nil default Git instance")
	}

	versionStr, err := g.RunGlobalOutput("version")
	if err != nil {
		t.Fatalf("RunGlobalOutput failed: %v", err)
	}
	if !strings.Contains(versionStr, "git version") {
		t.Errorf("expected 'git version', got %q", versionStr)
	}

	err = g.Run(tempDir, "init")
	if err != nil {
		t.Fatalf("git init failed: %v", err)
	}

	_ = g.Run(tempDir, "config", "local", "user.name", "Test User")
	_ = g.Run(tempDir, "config", "local", "user.email", "test@example.com")

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

	logOut, err := g.RunOutput(tempDir, "log", "-n", "1", "--oneline")
	if err != nil {
		t.Fatalf("git log failed: %v", err)
	}
	if !strings.Contains(logOut, "initial commit") {
		t.Errorf("log output missing message: %s", logOut)
	}

	res := g.RunSilent(tempDir, "status")
	if !strings.Contains(res, "nothing to commit") {
		t.Errorf("RunSilent status unexpected: %q", res)
	}

	patchStr, err := g.RunWithStdin(tempDir, []byte("line of content"), "hash-object", "-w", "--stdin")
	if err != nil {
		t.Fatalf("RunWithStdin failed: %v", err)
	}
	if len(patchStr) == 0 {
		t.Error("expected non-empty blob hash")
	}

	outEnv, err := g.RunOutputWithEnv(tempDir, map[string]string{"GIT_AUTHOR_NAME": "Override Author", "GIT_AUTHOR_EMAIL": "override@example.com"}, "var", "GIT_AUTHOR_IDENT")
	if err != nil {
		t.Fatalf("RunOutputWithEnv failed: %v", err)
	}
	if !strings.Contains(outEnv, "Override Author") {
		t.Errorf("expected 'Override Author' in GIT_AUTHOR_IDENT, got %q", outEnv)
	}

	err = g.RunWithEnv(tempDir, map[string]string{"GIT_AUTHOR_NAME": "Override Env", "GIT_AUTHOR_EMAIL": "override@example.com"}, "commit", "--allow-empty", "-m", "env commit")
	if err != nil {
		t.Fatalf("RunWithEnv failed: %v", err)
	}

	err = g.RunGlobal("version")
	if err != nil {
		t.Errorf("RunGlobal failed: %v", err)
	}
}

func TestDefaultErr(t *testing.T) {
	g, err := DefaultErr()
	if g == nil {
		t.Error("expected non-nil Git instance from DefaultErr")
	}
	if err != nil {
		t.Errorf("expected no error from DefaultErr, got %v", err)
	}
}

// TestDefaultGitNotFound exercises the error branch in Default() when git is
// not available in PATH. We temporarily reset the package-level singleton and
// clear PATH, then restore everything afterwards.
func TestDefaultGitNotFound(t *testing.T) {
	origInstance := defaultInstance
	origErr := defaultInitErr

	defaultOnce = sync.Once{}
	defaultInstance = nil
	defaultInitErr = nil

	defer func() {
		defaultOnce = sync.Once{}
		defaultInstance = origInstance
		defaultInitErr = origErr
	}()

	origPath := os.Getenv("PATH")
	t.Setenv("PATH", "")
	defer func() { _ = os.Setenv("PATH", origPath) }()

	g := Default()
	if g != nil {
		t.Error("expected nil Git instance when git is not in PATH")
	}

	g2, err2 := DefaultErr()
	if g2 != nil {
		t.Error("expected nil Git instance from DefaultErr when git is not in PATH")
	}
	if err2 == nil {
		t.Error("expected error from DefaultErr when git is not in PATH")
	}
	if err2 != nil && !strings.Contains(err2.Error(), "git CLI not found") {
		t.Errorf("expected 'git CLI not found' error, got: %v", err2)
	}
}

func TestRunOutputError(t *testing.T) {
	g := Default()
	if g == nil {
		t.Skip("git not available")
	}
	_, err := g.RunOutput("", "log", "--this-flag-does-not-exist-12345")
	if err == nil {
		t.Error("expected error from RunOutput with bad flag")
	}
}

func TestRunWithEnvError(t *testing.T) {
	g := Default()
	if g == nil {
		t.Skip("git not available")
	}
	err := g.RunWithEnv("", nil, "this-subcommand-does-not-exist")
	if err == nil {
		t.Error("expected error from RunWithEnv with bad subcommand")
	}
}

func TestRunOutputWithEnvError(t *testing.T) {
	g := Default()
	if g == nil {
		t.Skip("git not available")
	}
	_, err := g.RunOutputWithEnv("", nil, "this-subcommand-does-not-exist")
	if err == nil {
		t.Error("expected error from RunOutputWithEnv with bad subcommand")
	}
}

func TestWrapSSHError(t *testing.T) {
	tests := []struct {
		name     string
		inputErr error
		stderr   string
		wantNil  bool
		wantHint string
	}{
		{
			name:     "nil error passthrough",
			inputErr: nil,
			stderr:   "",
			wantNil:  true,
		},
		{
			name:     "non-SSH error passthrough",
			inputErr: errForTest("some git error"),
			stderr:   "fatal: something failed",
			wantHint: "",
		},
		{
			name:     "host key verification failed with host",
			inputErr: errForTest("ssh fail"),
			stderr:   "Host key verification failed for git@github.com\nfatal: could not read from remote",
			wantHint: "ssh -T git@github.com",
		},
		{
			name:     "known_hosts keyword without host",
			inputErr: errForTest("ssh fail"),
			stderr:   "Warning: permanently added to the list of known_hosts.\nno matching host key type",
			wantHint: "ssh -T git@<hostname>",
		},
		{
			name:     "no matching host key",
			inputErr: errForTest("ssh fail"),
			stderr:   "no matching host key type found",
			wantHint: "ssh -T git@<hostname>",
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			result := wrapSSHError(tc.inputErr, tc.stderr)
			if tc.wantNil {
				if result != nil {
					t.Fatalf("expected nil error, got %v", result)
				}
				return
			}
			if result == nil {
				t.Fatal("expected non-nil error")
			}
			if tc.wantHint != "" && !strings.Contains(result.Error(), tc.wantHint) {
				t.Errorf("expected hint containing %q, got: %s", tc.wantHint, result.Error())
			}
			if tc.wantHint == "" {
				if result.Error() != tc.inputErr.Error() {
					t.Errorf("expected passthrough error %q, got %q", tc.inputErr.Error(), result.Error())
				}
			}
		})
	}
}

func TestExtractHost(t *testing.T) {
	tests := []struct {
		name   string
		stderr string
		want   string
	}{
		{
			name:   "host key verification with user@host",
			stderr: "Host key verification failed for 'git@example.com'\nfatal: error",
			want:   "git@example.com",
		},
		{
			name:   "known_hosts line with user@host",
			stderr: "Warning: something about known_hosts for git@myhost.io\nother line",
			want:   "git@myhost.io",
		},
		{
			name:   "no host found",
			stderr: "Host key verification failed\nfatal: error",
			want:   "",
		},
		{
			name:   "no matching line",
			stderr: "fatal: error\nsomething else",
			want:   "",
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := extractHost(tc.stderr)
			if got != tc.want {
				t.Errorf("extractHost() = %q, want %q", got, tc.want)
			}
		})
	}
}

type simpleError string

func (e simpleError) Error() string { return string(e) }

func errForTest(msg string) error { return simpleError(msg) }

func TestCleanStderr(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want string
	}{
		{
			name: "empty string",
			raw:  "",
			want: "(no stderr output)",
		},
		{
			name: "whitespace only",
			raw:  "   \n  \n  ",
			want: "(no stderr output)",
		},
		{
			name: "single meaningful line",
			raw:  "fatal: not a git repository",
			want: "fatal: not a git repository",
		},
		{
			name: "progress lines only with last line non-empty",
			raw:  "Counting objects: 100\nCompressing objects: 50%\nsome trailing info",
			want: "some trailing info",
		},
		{
			name: "progress lines only - last line is progress",
			raw:  "Counting objects: 100\nCompressing objects: 50%",
			want: "Compressing objects: 50%",
		},
		{
			name: "progress lines only with trailing whitespace",
			raw:  "Counting objects: 100\nCompressing objects: 50%\n   ",
			want: "Compressing objects: 50%",
		},
		{
			name: "more than 3 meaningful lines",
			raw:  "line1\nline2\nline3\nline4\nline5",
			want: "line3; line4; line5",
		},
		{
			name: "meaningful lines with internal empty lines",
			raw:  "fatal: error\n\n\nwarning: something",
			want: "fatal: error; warning: something",
		},
		{
			name: "mixed progress and meaningful",
			raw:  "Counting objects: 100\nfatal: error\nReceiving objects: 50%\nsome detail",
			want: "fatal: error; some detail",
		},
		{
			name: "all progress only but trailing non-empty line",
			raw:  "Counting objects: 100%\nReceiving objects: 100%\nremote: Total 42",
			want: "remote: Total 42",
		},
		{
			name: "progress only with empty lines between - hits fallback",
			raw:  "Counting objects: 100",
			want: "Counting objects: 100",
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := CleanStderr(tc.raw)
			if got != tc.want {
				t.Errorf("CleanStderr() = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestIsProgressLine(t *testing.T) {
	tests := []struct {
		line string
		want bool
	}{
		{"Counting objects: 100", true},
		{"Compressing objects: 50%", true},
		{"Receiving objects: 100% (15/15)", true},
		{"Resolving deltas: 100% (5/5)", true},
		{"remote: Counting objects: 10", true},
		{"remote: Compressing objects: 100%", true},
		{"remote: Total 42", true},
		{"fatal: not a git repository", false},
		{"error: something happened", false},
		{"", false},
	}

	for _, tc := range tests {
		t.Run(tc.line, func(t *testing.T) {
			got := IsProgressLine(tc.line)
			if got != tc.want {
				t.Errorf("IsProgressLine(%q) = %v, want %v", tc.line, got, tc.want)
			}
		})
	}
}

func TestMapToEnv(t *testing.T) {
	m := map[string]string{
		"KEY1": "val1",
		"KEY2": "val2",
	}
	result := MapToEnv(m)
	if len(result) != 2 {
		t.Fatalf("expected 2 entries, got %d", len(result))
	}
	found := make(map[string]bool)
	for _, s := range result {
		found[s] = true
	}
	if !found["KEY1=val1"] || !found["KEY2=val2"] {
		t.Errorf("unexpected MapToEnv result: %v", result)
	}
}

func TestHookBlockMarker(t *testing.T) {
	marker := hookBlockMarker()
	if marker == "" {
		t.Error("hookBlockMarker returned empty string")
	}
	if !strings.HasSuffix(marker, " HOOK") {
		t.Errorf("expected marker ending with ' HOOK', got %q", marker)
	}
}

func TestNewHookManager(t *testing.T) {
	dir := t.TempDir()

	hm := NewHookManager(dir)
	if hm.projectDir != dir {
		t.Errorf("expected projectDir=%q, got %q", dir, hm.projectDir)
	}
	if hm.hooksDir == "" {
		t.Error("hooksDir should not be empty")
	}
}

func TestNewHookManagerEmptyDir(t *testing.T) {
	hm := NewHookManager("")
	wd, _ := os.Getwd()
	if hm.projectDir != wd {
		t.Errorf("expected projectDir=%q (cwd), got %q", wd, hm.projectDir)
	}
}

func TestHookManagerInstallNoGitDir(t *testing.T) {
	dir := t.TempDir()
	hm := NewHookManager(dir)
	err := hm.Install(false)
	if err != nil {
		t.Fatalf("Install should succeed without .git dir, got %v", err)
	}
}

func TestHookManagerInstallAndRemove(t *testing.T) {
	dir := t.TempDir()
	gitDir := filepath.Join(dir, ".git")
	if err := os.MkdirAll(gitDir, 0o755); err != nil {
		t.Fatalf("failed to create .git dir: %v", err)
	}

	hm := NewHookManager(dir)
	err := hm.Install(false)
	if err != nil {
		t.Fatalf("Install failed: %v", err)
	}

	for _, hookType := range []HookType{PostCommit, PrePush, PostMerge} {
		hookPath := filepath.Join(hm.hooksDir, string(hookType))
		data, err := os.ReadFile(hookPath)
		if err != nil {
			t.Errorf("expected hook file %s to exist: %v", hookType, err)
			continue
		}
		if !strings.Contains(string(data), hookBlockMarker()) {
			t.Errorf("hook %s missing block marker", hookType)
		}
	}

	err = hm.Remove()
	if err != nil {
		t.Fatalf("Remove failed: %v", err)
	}

	for _, hookType := range []HookType{PostCommit, PrePush, PostMerge} {
		hookPath := filepath.Join(hm.hooksDir, string(hookType))
		if _, err := os.Stat(hookPath); err == nil {
			t.Errorf("expected hook %s to be removed", hookType)
		}
	}
}

func TestHookManagerRemoveNoGitDir(t *testing.T) {
	dir := t.TempDir()
	hm := NewHookManager(dir)
	err := hm.Remove()
	if err != nil {
		t.Fatalf("Remove should succeed without .git dir, got %v", err)
	}
}

func TestHookScript(t *testing.T) {
	script := hookScript("test comment")
	if script == "" {
		t.Error("hookScript returned empty string")
	}
	if !strings.Contains(script, "test comment") {
		t.Error("hookScript should contain the comment")
	}
	if !strings.Contains(script, "sync") {
		t.Error("hookScript should contain sync command")
	}
	if !strings.Contains(script, "--debounce "+hookDebounce) {
		t.Errorf("hookScript should debounce the sync, got: %q", script)
	}
	if _, err := time.ParseDuration(hookDebounce); err != nil {
		t.Errorf("hookDebounce %q is not a duration the sync command can parse: %v", hookDebounce, err)
	}
}

func TestBinPath(t *testing.T) {
	p := binPath()
	if p == "" {
		t.Error("binPath returned empty string")
	}
}

func TestHasNonShellShebang(t *testing.T) {
	tests := []struct {
		name string
		data string
		want bool
	}{
		{"empty", "", false},
		{"no shebang", "echo hello", false},
		{"sh shebang", "#!/bin/sh\necho hello", false},
		{"bash shebang", "#!/bin/bash\necho hello", false},
		{"env sh", "#!/usr/bin/env sh\necho hello", false},
		{"env bash", "#!/usr/bin/env bash\necho hello", false},
		{"python shebang", "#!/usr/bin/env python3\nprint('hi')", true},
		{"node shebang", "#!/usr/bin/env node\nconsole.log('hi')", true},
		{"ruby shebang", "#!/usr/bin/ruby\nputs 'hi'", true},
		{"shebang only no newline", "#!/bin/sh", false},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := hasNonShellShebang([]byte(tc.data))
			if got != tc.want {
				t.Errorf("hasNonShellShebang(%q) = %v, want %v", tc.data, got, tc.want)
			}
		})
	}
}

func TestInstallSkipsNonShellShebang(t *testing.T) {
	dir := t.TempDir()
	gitDir := filepath.Join(dir, ".git")
	hooksDir := filepath.Join(gitDir, "hooks")
	if err := os.MkdirAll(hooksDir, 0o755); err != nil {
		t.Fatalf("failed to create hooks dir: %v", err)
	}

	hookPath := filepath.Join(hooksDir, string(PostCommit))
	_ = os.WriteFile(hookPath, []byte("#!/usr/bin/env python3\nprint('hook')"), 0755)

	hm := NewHookManager(dir)
	err := hm.Install(false)
	if err != nil {
		t.Fatalf("Install failed: %v", err)
	}

	data, _ := os.ReadFile(hookPath)
	if strings.Contains(string(data), hookBlockMarker()) {
		t.Error("Install should have skipped hook with non-shell shebang")
	}
}

func TestResolveGitDir(t *testing.T) {
	t.Run("regular .git dir", func(t *testing.T) {
		dir := t.TempDir()
		gitDir := filepath.Join(dir, ".git")
		_ = os.MkdirAll(gitDir, 0o755)
		result := resolveGitDir(dir)
		if result != gitDir {
			t.Errorf("expected %q, got %q", gitDir, result)
		}
	})

	t.Run("no .git", func(t *testing.T) {
		dir := t.TempDir()
		result := resolveGitDir(dir)
		expected := filepath.Join(dir, ".git")
		if result != expected {
			t.Errorf("expected %q, got %q", expected, result)
		}
	})

	t.Run(".git file (worktree)", func(t *testing.T) {
		dir := t.TempDir()
		actualGitDir := filepath.Join(dir, "actual-gitdir")
		_ = os.MkdirAll(actualGitDir, 0o755)

		dotGit := filepath.Join(dir, ".git")
		_ = os.WriteFile(dotGit, []byte("gitdir: "+actualGitDir+"\n"), 0644)

		result := resolveGitDir(dir)
		if result != filepath.Clean(actualGitDir) {
			t.Errorf("expected %q, got %q", filepath.Clean(actualGitDir), result)
		}
	})

	t.Run(".git file with relative path", func(t *testing.T) {
		dir := t.TempDir()
		actualGitDir := filepath.Join(dir, "sub", "gitdir")
		_ = os.MkdirAll(actualGitDir, 0o755)

		dotGit := filepath.Join(dir, ".git")
		_ = os.WriteFile(dotGit, []byte("gitdir: sub/gitdir\n"), 0644)

		result := resolveGitDir(dir)
		expected := filepath.Clean(filepath.Join(dir, "sub", "gitdir"))
		if result != expected {
			t.Errorf("expected %q, got %q", expected, result)
		}
	})

	t.Run(".git file with invalid prefix", func(t *testing.T) {
		dir := t.TempDir()
		dotGit := filepath.Join(dir, ".git")
		_ = os.WriteFile(dotGit, []byte("not-gitdir-prefix\n"), 0644)

		result := resolveGitDir(dir)
		if result != dotGit {
			t.Errorf("expected fallback %q, got %q", dotGit, result)
		}
	})
}

func TestIsShellShebangOnly(t *testing.T) {
	tests := []struct {
		input string
		want  bool
	}{
		{"", true},
		{"#!/bin/sh", true},
		{"#!/bin/bash", true},
		{"#!/usr/bin/env sh", true},
		{"#!/usr/bin/env bash", true},
		{"  #!/bin/sh  ", true},
		{"#!/bin/sh\necho hi", false},
		{"#!/usr/bin/env python", false},
		{"something else", false},
	}

	for _, tc := range tests {
		t.Run(tc.input, func(t *testing.T) {
			got := isShellShebangOnly(tc.input)
			if got != tc.want {
				t.Errorf("isShellShebangOnly(%q) = %v, want %v", tc.input, got, tc.want)
			}
		})
	}
}

func TestInjectBlockStyledExistingContentNoShebang(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	_ = os.WriteFile(filePath, []byte("some existing content\n"), 0644)

	err := InjectBlockStyled(filePath, "block content", "MARKER", "", ShellBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	content := string(data)
	if !strings.Contains(content, "some existing content") {
		t.Error("existing content should be preserved")
	}
	if !strings.Contains(content, "# --- MARKER ---") {
		t.Error("block should be injected")
	}
}

func TestInjectBlockStyledReplaceExistingBlock(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	existing := "#!/bin/sh\n\n\n\n# --- M1 ---\nold content\n# --- END M1 ---\n\n\n\nother stuff\n"
	_ = os.WriteFile(filePath, []byte(existing), 0644)

	err := InjectBlockStyled(filePath, "new content", "M1", "#!/bin/sh", ShellBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	content := string(data)
	if strings.Contains(content, "old content") {
		t.Error("old block content should be replaced")
	}
	if !strings.Contains(content, "new content") {
		t.Error("new block content should be present")
	}
}

func TestInjectBlockStyledNoShebangEmptyFile(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.txt")

	err := InjectBlockStyled(filePath, "hello", "BLOCK1", "", ShellBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	expected := "# --- BLOCK1 ---\nhello\n# --- END BLOCK1 ---\n"
	if string(data) != expected {
		t.Errorf("expected:\n%q\ngot:\n%q", expected, string(data))
	}
}

func TestRemoveBlockStyledNoChange(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	_ = os.WriteFile(filePath, []byte("#!/bin/sh\necho hello\n"), 0644)

	removed, err := RemoveBlockStyled(filePath, "NONEXISTENT", false, ShellBlockStyle)
	if err != nil {
		t.Fatalf("RemoveBlockStyled failed: %v", err)
	}
	if removed {
		t.Error("expected no removal when block not found")
	}
}

func TestRemoveBlockStyledFileNotFound(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "does-not-exist.sh")

	removed, err := RemoveBlockStyled(filePath, "M1", false, ShellBlockStyle)
	if err != nil {
		t.Fatalf("RemoveBlockStyled should not error on missing file, got %v", err)
	}
	if removed {
		t.Error("expected no removal when file doesn't exist")
	}
}

func TestRemoveBlockStyledCleanedNonEmpty(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	content := "#!/bin/sh\necho preserve me\n\n# --- RM1 ---\nblock stuff\n# --- END RM1 ---\n"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	removed, err := RemoveBlockStyled(filePath, "RM1", true, ShellBlockStyle)
	if err != nil {
		t.Fatalf("RemoveBlockStyled failed: %v", err)
	}
	if !removed {
		t.Error("expected block to be removed")
	}

	data, err := os.ReadFile(filePath)
	if err != nil {
		t.Fatalf("file should still exist: %v", err)
	}
	if !strings.Contains(string(data), "preserve me") {
		t.Error("non-block content should be preserved")
	}
}

func TestInjectBlockStyledShebangOnlyInExisting(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	_ = os.WriteFile(filePath, []byte("#!/bin/bash\n"), 0644)

	err := InjectBlockStyled(filePath, "new block", "MK", "#!/bin/sh", ShellBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	content := string(data)
	if !strings.Contains(content, "# --- MK ---") {
		t.Error("block should be injected")
	}
}

func TestInjectBlockStyledWriteError(t *testing.T) {
	err := InjectBlockStyled("/proc/nonexistent/path/file", "content", "M1", "", ShellBlockStyle)
	if err == nil {
		t.Error("expected error when writing to invalid path")
	}
}

func TestRemoveBlockStyledDeleteError(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	_ = os.WriteFile(filePath, []byte("#!/bin/sh\n# --- DM ---\nhello\n# --- END DM ---\n"), 0644)

	_ = os.Chmod(dir, 0o555)
	defer func() { _ = os.Chmod(dir, 0o755) }()

	_, err := RemoveBlockStyled(filePath, "DM", true, ShellBlockStyle)
	if err == nil {
		t.Error("expected error when removing file from read-only directory")
	}
}

func TestRemoveBlockStyledWriteError(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	content := "echo keep\n# --- WE ---\nstuff\n# --- END WE ---\n"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	_ = os.Chmod(filePath, 0o444)
	defer func() { _ = os.Chmod(filePath, 0o644) }()

	_, err := RemoveBlockStyled(filePath, "WE", false, ShellBlockStyle)
	if err == nil {
		t.Error("expected error when writing to read-only file")
	}
}

func TestInjectBlockStyledUpdateWriteError(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	content := "# --- UW ---\nold\n# --- END UW ---\n"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	_ = os.Chmod(filePath, 0o444)
	defer func() { _ = os.Chmod(filePath, 0o644) }()

	err := InjectBlockStyled(filePath, "new", "UW", "", ShellBlockStyle)
	if err == nil {
		t.Error("expected error when writing to read-only file")
	}
}

func TestInjectBlockStyledExistingBlockLeadingTrailingNewlines(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	content := "some content\n\n\n\n\n# --- LT ---\nold content\n# --- END LT ---\n\n\n\n\nmore content\n"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	err := InjectBlockStyled(filePath, "new content", "LT", "", ShellBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	result := string(data)
	if !strings.Contains(result, "new content") {
		t.Error("new content should be present")
	}
	if strings.Contains(result, "\n\n\n") {
		t.Error("should not have 3+ consecutive newlines after normalization")
	}
}

func TestInjectBlockStyledEmptyResultAfterUpdate(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	content := "# --- O ---\nold\n# --- END O ---\n"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	err := InjectBlockStyled(filePath, "new", "O", "", ShellBlockStyle)
	if err != nil {
		t.Fatalf("InjectBlockStyled failed: %v", err)
	}

	data, _ := os.ReadFile(filePath)
	if !strings.Contains(string(data), "new") {
		t.Error("updated content should be present")
	}
}

func TestRemoveBlockStyledEmptyResultWithDeleteIfEmpty(t *testing.T) {
	dir := t.TempDir()
	filePath := filepath.Join(dir, "test.sh")

	content := "# --- ONLY ---\nsome stuff\n# --- END ONLY ---\n"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	removed, err := RemoveBlockStyled(filePath, "ONLY", false, ShellBlockStyle)
	if err != nil {
		t.Fatalf("RemoveBlockStyled failed: %v", err)
	}
	if !removed {
		t.Error("expected block to be removed")
	}

	data, err := os.ReadFile(filePath)
	if err != nil {
		t.Fatalf("file should still exist: %v", err)
	}
	if strings.TrimSpace(string(data)) != "" {
		t.Errorf("expected empty file, got %q", string(data))
	}
}

func TestResolveGitDirUnreadableFile(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("chmod not effective on Windows")
	}
	dir := t.TempDir()
	dotGit := filepath.Join(dir, ".git")
	_ = os.WriteFile(dotGit, []byte("gitdir: /some/path"), 0644)
	_ = os.Chmod(dotGit, 0o000)
	defer func() { _ = os.Chmod(dotGit, 0o644) }()

	result := resolveGitDir(dir)
	if result != dotGit {
		t.Errorf("expected fallback %q, got %q", dotGit, result)
	}
}

func TestHookManagerInstallMkdirAllError(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("chmod not effective on Windows")
	}
	dir := t.TempDir()
	gitDir := filepath.Join(dir, ".git")
	_ = os.MkdirAll(gitDir, 0o755)

	hm := NewHookManager(dir)
	_ = os.Chmod(gitDir, 0o444)
	defer func() { _ = os.Chmod(gitDir, 0o755) }()

	err := hm.Install(false)
	if err == nil {
		t.Error("expected error from Install when MkdirAll fails")
	}
	if !strings.Contains(err.Error(), "hooks: create dir") {
		t.Errorf("expected 'hooks: create dir' error, got: %v", err)
	}
}

func TestHookManagerInstallInjectBlockError(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("chmod not effective on Windows")
	}
	dir := t.TempDir()
	gitDir := filepath.Join(dir, ".git")
	hooksDir := filepath.Join(gitDir, "hooks")
	_ = os.MkdirAll(hooksDir, 0o755)

	hm := NewHookManager(dir)
	_ = os.Chmod(hooksDir, 0o555)
	defer func() { _ = os.Chmod(hooksDir, 0o755) }()

	err := hm.Install(false)
	if err == nil {
		t.Error("expected error from Install when InjectBlock fails")
	}
	if !strings.Contains(err.Error(), "hooks: inject") {
		t.Errorf("expected 'hooks: inject' error, got: %v", err)
	}
}

func TestHookManagerRemoveBlockError(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("chmod not effective on Windows")
	}
	dir := t.TempDir()
	gitDir := filepath.Join(dir, ".git")
	hooksDir := filepath.Join(gitDir, "hooks")
	_ = os.MkdirAll(hooksDir, 0o755)

	hm := NewHookManager(dir)

	if err := hm.Install(false); err != nil {
		t.Fatalf("Install failed: %v", err)
	}

	_ = os.Chmod(hooksDir, 0o555)
	defer func() { _ = os.Chmod(hooksDir, 0o755) }()

	err := hm.Remove()
	if err == nil {
		t.Error("expected error from Remove when RemoveBlock fails")
	}
	if !strings.Contains(err.Error(), "hooks: remove") {
		t.Errorf("expected 'hooks: remove' error, got: %v", err)
	}
}

func TestBinPathNonWindows(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("testing non-windows path")
	}
	p := binPath()
	if strings.HasSuffix(p, ".exe") {
		t.Errorf("binPath on non-windows should not end with .exe, got %q", p)
	}
}
