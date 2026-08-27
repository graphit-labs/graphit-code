package git

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type HookType string

const (
	PostCommit HookType = "post-commit"
	PrePush    HookType = "pre-push"
	PostMerge  HookType = "post-merge"
)

func hookBlockMarker() string { return strings.ToUpper(brand.Brand) + " HOOK" }

type HookManager struct {
	projectDir string
	hooksDir   string
}

func NewHookManager(projectDir string) *HookManager {
	if projectDir == "" {
		projectDir, _ = os.Getwd()
	}
	return &HookManager{
		projectDir: projectDir,
		hooksDir:   filepath.Join(resolveGitDir(projectDir), "hooks"),
	}
}

func resolveGitDir(projectDir string) string {
	dotGit := filepath.Join(projectDir, ".git")
	info, err := os.Lstat(dotGit)
	if err != nil {
		return dotGit
	}
	if info.IsDir() {
		return dotGit
	}

	data, err := os.ReadFile(dotGit)
	if err != nil {
		return dotGit
	}
	line := strings.TrimSpace(string(data))
	if !strings.HasPrefix(line, "gitdir: ") {
		return dotGit
	}
	gitdir := strings.TrimPrefix(line, "gitdir: ")
	if !filepath.IsAbs(gitdir) {
		gitdir = filepath.Join(projectDir, gitdir)
	}
	return filepath.Clean(gitdir)
}

func (h *HookManager) Install(_ bool) error {

	dotGit := filepath.Join(h.projectDir, ".git")
	if _, err := os.Lstat(dotGit); os.IsNotExist(err) {
		return nil
	}

	if err := os.MkdirAll(h.hooksDir, 0o755); err != nil {
		return fmt.Errorf("hooks: create dir: %w", err)
	}

	marker := hookBlockMarker()
	shebang := "#!/usr/bin/env sh"

	hooks := map[HookType]string{
		PostCommit: hookScript("sync after commit (silent, non-blocking)"),
		PrePush:    hookScript("sync before push (silent, non-blocking)"),
		PostMerge:  hookScript("sync after merge (silent, non-blocking)"),
	}

	for hookType, content := range hooks {
		path := filepath.Join(h.hooksDir, string(hookType))

		if data, err := os.ReadFile(path); err == nil {
			if hasNonShellShebang(data) {
				continue
			}
		}

		if err := InjectBlock(path, content, marker, shebang); err != nil {
			return fmt.Errorf("hooks: inject %s: %w", hookType, err)
		}
	}
	return nil
}

func (h *HookManager) Remove() error {

	dotGit := filepath.Join(h.projectDir, ".git")
	if _, err := os.Lstat(dotGit); os.IsNotExist(err) {
		return nil
	}

	marker := hookBlockMarker()
	hooks := []HookType{PostCommit, PrePush, PostMerge}
	for _, hookType := range hooks {
		path := filepath.Join(h.hooksDir, string(hookType))
		if _, err := RemoveBlock(path, marker, true); err != nil {
			return fmt.Errorf("hooks: remove %s: %w", hookType, err)
		}
	}
	return nil
}

// hookDebounce is how recently a sync must have finished for a hook-triggered one to
// stand down.
//
// The three hooks below fire on events that arrive together over a tree that changed
// once: commit, then push, then — on the other side of a pull — merge. Each used to
// run a full reindex, so a routine commit-and-push cost two of them concurrently, on
// top of whatever the daemon was already doing about the same file writes. The window
// only ever suppresses a sync that a completed one has already covered.
const hookDebounce = "60s"

func hookScript(comment string) string {
	bin := binPath()
	lines := []string{
		"# " + brand.DisplayName + " — " + comment,
		"command -v " + bin + " >/dev/null 2>&1 || exit 0",
		"(" + bin + " sync --debounce " + hookDebounce + " </dev/null >/dev/null 2>&1 &)",
	}
	return strings.Join(lines, "\n")
}

func binPath() string {
	if runtime.GOOS == "windows" {
		return brand.BinNameWindows()
	}
	return brand.BinName()
}

func hasNonShellShebang(data []byte) bool {
	s := strings.TrimSpace(string(data))
	if !strings.HasPrefix(s, "#!") {
		return false
	}

	line := s
	if idx := strings.IndexByte(s, '\n'); idx >= 0 {
		line = s[:idx]
	}
	line = strings.TrimSpace(line)

	for _, shell := range []string{"/bin/sh", "/bin/bash", "env sh", "env bash"} {
		if strings.Contains(line, shell) {
			return false
		}
	}

	return true
}
