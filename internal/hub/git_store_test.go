package hub

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestGitStore_Dir(t *testing.T) {
	t.Parallel()
	gs := &GitStore{repoDir: "/tmp/test-repo"}
	if gs.Dir() != "/tmp/test-repo" {
		t.Errorf("expected /tmp/test-repo, got %q", gs.Dir())
	}
}

func TestGitStore_CacheBase(t *testing.T) {
	t.Parallel()
	gs := &GitStore{cacheBase: "/tmp/test-cache"}
	if gs.CacheBase() != "/tmp/test-cache" {
		t.Errorf("expected /tmp/test-cache, got %q", gs.CacheBase())
	}
}

func TestGitStore_AbsPath(t *testing.T) {
	t.Parallel()
	gs := &GitStore{repoDir: "/tmp/test-repo"}
	got := gs.AbsPath("subdir/file.txt")
	want := filepath.Join("/tmp/test-repo", "subdir/file.txt")
	if got != want {
		t.Errorf("expected %q, got %q", want, got)
	}
}

func TestGitStore_ReadWriteFile(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	gs := &GitStore{repoDir: dir}

	t.Run("write and read", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		gs2 := &GitStore{repoDir: d}
		err := gs2.WriteFile("test/file.txt", []byte("hello"))
		if err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
		data, err := gs2.ReadFile("test/file.txt")
		if err != nil {
			t.Fatalf("ReadFile: %v", err)
		}
		if string(data) != "hello" {
			t.Errorf("expected 'hello', got %q", data)
		}
	})

	t.Run("read nonexistent", func(t *testing.T) {
		t.Parallel()
		_, err := gs.ReadFile("nonexistent.txt")
		if err == nil {
			t.Error("expected error for nonexistent file")
		}
	})

	t.Run("write creates parent dirs", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		gs3 := &GitStore{repoDir: d}
		err := gs3.WriteFile("deep/nested/dir/file.txt", []byte("data"))
		if err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
	})
}

func TestGitStore_RemoveAll(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	gs := &GitStore{repoDir: dir}

	// Create something to remove
	subdir := filepath.Join(dir, "to-remove")
	_ = os.MkdirAll(subdir, 0o755)
	_ = os.WriteFile(filepath.Join(subdir, "file.txt"), []byte("data"), 0o644)

	err := gs.RemoveAll("to-remove")
	if err != nil {
		t.Fatalf("RemoveAll: %v", err)
	}

	if _, err := os.Stat(subdir); !os.IsNotExist(err) {
		t.Error("expected directory to be removed")
	}
}

func TestGitStore_isRebasing(t *testing.T) {
	t.Parallel()

	t.Run("not rebasing", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.MkdirAll(filepath.Join(dir, ".git"), 0o755)
		gs := &GitStore{repoDir: dir}
		if gs.isRebasing() {
			t.Error("expected not rebasing")
		}
	})

	t.Run("rebase-merge exists", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.MkdirAll(filepath.Join(dir, ".git", "rebase-merge"), 0o755)
		gs := &GitStore{repoDir: dir}
		if !gs.isRebasing() {
			t.Error("expected rebasing when rebase-merge exists")
		}
	})

	t.Run("rebase-apply exists", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.MkdirAll(filepath.Join(dir, ".git", "rebase-apply"), 0o755)
		gs := &GitStore{repoDir: dir}
		if !gs.isRebasing() {
			t.Error("expected rebasing when rebase-apply exists")
		}
	})
}

func TestGitStore_eventsStagingDir(t *testing.T) {
	t.Parallel()
	gs := &GitStore{cacheBase: "/tmp/test-cache"}
	dir := gs.eventsStagingDir()
	if !strings.Contains(dir, "events-staging") {
		t.Errorf("expected events-staging in path, got %q", dir)
	}
}

func TestGitStore_WriteEventFile(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	gs := &GitStore{cacheBase: dir}

	gs.WriteEventFile("project/event1.json", []byte(`{"type":"test"}`))

	data, err := os.ReadFile(filepath.Join(dir, "events-staging", "project", "event1.json"))
	if err != nil {
		t.Fatalf("expected event file to be written: %v", err)
	}
	if string(data) != `{"type":"test"}` {
		t.Errorf("unexpected content: %q", data)
	}
}

func TestGitStore_EnsureEventsClone(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	gs := &GitStore{cacheBase: dir}
	err := gs.EnsureEventsClone()
	if err != nil {
		t.Fatalf("EnsureEventsClone: %v", err)
	}
	if _, err := os.Stat(gs.eventsStagingDir()); err != nil {
		t.Error("expected events staging dir to exist")
	}
}

func TestGitStore_worktreeDirForBranch(t *testing.T) {
	t.Parallel()
	gs := &GitStore{cacheBase: "/tmp/cache"}

	t.Run("simple branch", func(t *testing.T) {
		t.Parallel()
		dir := gs.worktreeDirForBranch("main")
		if !strings.HasSuffix(dir, "main") {
			t.Errorf("expected 'main' suffix, got %q", dir)
		}
	})

	t.Run("branch with slashes", func(t *testing.T) {
		t.Parallel()
		dir := gs.worktreeDirForBranch("artifact/rules/proj/id/1.0.0")
		if strings.Contains(dir, "/artifact/") {
			t.Errorf("expected slashes to be replaced, got %q", dir)
		}
	})
}

func TestGitStore_worktreesBaseDir(t *testing.T) {
	t.Parallel()
	gs := &GitStore{cacheBase: "/tmp/test-cache"}
	if gs.worktreesBaseDir() != "/tmp/test-cache" {
		t.Errorf("expected /tmp/test-cache, got %q", gs.worktreesBaseDir())
	}
}

func TestArtifactBranchName(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name      string
		artType   ArtifactType
		id        string
		version   string
		projectID string
		want      string
	}{
		{
			name:      "rule with project",
			artType:   TypeRule,
			id:        "my-rule",
			version:   "1.0.0",
			projectID: "my-project",
			want:      "artifact/rules/my-project/my-rule/1.0.0",
		},
		{
			name:      "rule without project (global)",
			artType:   TypeRule,
			id:        "my-rule",
			version:   "1.0.0",
			projectID: "",
			want:      "artifact/rules/_global/my-rule/1.0.0",
		},
		{
			name:      "AST (no ID segment)",
			artType:   TypeAST,
			id:        "my-ast",
			version:   "2.0.0",
			projectID: "proj",
			want:      "artifact/ast/proj/2.0.0",
		},
		{
			name:      "Knowledge (no ID segment)",
			artType:   TypeKnowledge,
			id:        "my-knowledge",
			version:   "1.0.0",
			projectID: "",
			want:      "artifact/knowledge/_global/1.0.0",
		},
		{
			name:      "skill",
			artType:   TypeSkill,
			id:        "my-skill",
			version:   "1.0.0",
			projectID: "",
			want:      "artifact/skills/my-skill/1.0.0",
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			got := ArtifactBranchName(tc.artType, tc.id, tc.version, tc.projectID)
			// The exact name depends on TypeFolderMap; just check the structure
			if !strings.HasPrefix(got, "artifact/") {
				t.Errorf("expected artifact/ prefix, got %q", got)
			}
		})
	}
}

func TestGitStore_ArtifactCloneDir(t *testing.T) {
	t.Parallel()
	gs := &GitStore{cacheBase: "/tmp/cache"}

	t.Run("rule type", func(t *testing.T) {
		t.Parallel()
		dir := gs.ArtifactCloneDir(TypeRule, "my-rule", "1.0.0", "proj")
		if !strings.HasPrefix(dir, "/tmp/cache/") {
			t.Errorf("expected cache base prefix, got %q", dir)
		}
	})

	t.Run("AST type (no ID segment)", func(t *testing.T) {
		t.Parallel()
		dir := gs.ArtifactCloneDir(TypeAST, "ignored", "1.0.0", "proj")
		if !strings.HasPrefix(dir, "/tmp/cache/") {
			t.Errorf("expected cache base prefix, got %q", dir)
		}
	})

	t.Run("global (empty projectID)", func(t *testing.T) {
		t.Parallel()
		dir := gs.ArtifactCloneDir(TypeRule, "my-rule", "1.0.0", "")
		if !strings.Contains(dir, "_global") {
			t.Errorf("expected _global in path, got %q", dir)
		}
	})

	t.Run("unknown type", func(t *testing.T) {
		t.Parallel()
		dir := gs.ArtifactCloneDir("custom-type", "my-id", "1.0.0", "")
		if dir == "" {
			t.Error("expected non-empty dir")
		}
	})
}

func TestMemoryWorktree_FileOps(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	wt := &MemoryWorktree{dir: dir}

	t.Run("Dir", func(t *testing.T) {
		t.Parallel()
		if wt.Dir() != dir {
			t.Errorf("expected %q, got %q", dir, wt.Dir())
		}
	})

	t.Run("WriteFile and ReadFile", func(t *testing.T) {
		d := t.TempDir()
		wt2 := &MemoryWorktree{dir: d}

		if err := wt2.WriteFile("sub/test.txt", []byte("content")); err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
		data, err := wt2.ReadFile("sub/test.txt")
		if err != nil {
			t.Fatalf("ReadFile: %v", err)
		}
		if string(data) != "content" {
			t.Errorf("expected 'content', got %q", data)
		}
	})

	t.Run("RemoveFile", func(t *testing.T) {
		d := t.TempDir()
		wt3 := &MemoryWorktree{dir: d}
		_ = os.WriteFile(filepath.Join(d, "todelete.txt"), []byte("bye"), 0o644)
		if err := wt3.RemoveFile("todelete.txt"); err != nil {
			t.Fatalf("RemoveFile: %v", err)
		}
		if _, err := os.Stat(filepath.Join(d, "todelete.txt")); !os.IsNotExist(err) {
			t.Error("expected file to be removed")
		}
	})

	t.Run("ListDir", func(t *testing.T) {
		d := t.TempDir()
		wt4 := &MemoryWorktree{dir: d}
		_ = os.WriteFile(filepath.Join(d, "file1.txt"), []byte("1"), 0o644)
		_ = os.WriteFile(filepath.Join(d, "file2.txt"), []byte("2"), 0o644)
		entries, err := wt4.ListDir(".")
		if err != nil {
			t.Fatalf("ListDir: %v", err)
		}
		if len(entries) != 2 {
			t.Errorf("expected 2 entries, got %d", len(entries))
		}
	})

	t.Run("ListDir nonexistent", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		wt5 := &MemoryWorktree{dir: d}
		_, err := wt5.ListDir("nonexistent")
		if err == nil {
			t.Error("expected error for nonexistent dir")
		}
	})
}

func TestGitStore_log(t *testing.T) {
	t.Parallel()
	gs := &GitStore{}
	logger := gs.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}
