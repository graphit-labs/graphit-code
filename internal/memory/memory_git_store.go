package memory

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/config"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

type MemoryGitStore struct {
	repoDir string
	wtBase  string
}

func NewMemoryGitStore() (*MemoryGitStore, error) {
	repoDir, err := config.MemoryRepoDirPath()
	if err != nil {
		return nil, fmt.Errorf("resolving memory repo path: %w", err)
	}

	wtBase := repoDir + "-wt"
	return &MemoryGitStore{repoDir: repoDir, wtBase: wtBase}, nil
}

func (m *MemoryGitStore) Dir() string { return m.repoDir }

func (m *MemoryGitStore) EnsureInitialised() error {
	if err := os.MkdirAll(m.repoDir, 0o755); err != nil {
		return fmt.Errorf("creating memory repo dir: %w", err)
	}

	if _, err := os.Stat(filepath.Join(m.repoDir, ".git")); os.IsNotExist(err) {
		if err := m.git("init", "--initial-branch=main", m.repoDir); err != nil {
			return fmt.Errorf("initialising memory git repo: %w", err)
		}

		if err := m.bootstrapInitialCommit(); err != nil {
			return fmt.Errorf("bootstrap memory repo: %w", err)
		}
	}

	if err := m.gitInRepo("config", "fetch.depth", "1"); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] git config fetch.depth: %v\n", err)
	}

	m.syncRemote()

	if out, err := m.gitOutputInRepo("for-each-ref", "--format=%(refname)", "refs/remotes/origin/"); err == nil {
		for _, ref := range strings.Split(out, "\n") {
			ref = strings.TrimSpace(ref)
			if ref != "" && ref != "refs/remotes/origin/main" {
				if err := m.gitInRepo("update-ref", "-d", ref); err != nil {
					fmt.Fprintf(os.Stderr, "[memory] prune stale ref %s: %v\n", ref, err)
				}
			}
		}
	}

	return nil
}

func (m *MemoryGitStore) syncRemote() {
	remoteURL := config.MemoryRepoURL()
	if remoteURL == "" {

		_ = m.gitInRepo("remote", "remove", "origin") // ignore error
		return
	}

	out, _ := m.gitOutputInRepo("remote", "get-url", "origin")
	currentURL := strings.TrimSpace(out)

	if currentURL == remoteURL {

		_ = m.gitInRepo("config", "remote.origin.fetch", "+refs/heads/main:refs/remotes/origin/main")
		return
	}

	if currentURL == "" {

		if err := m.gitInRepo("remote", "add", "origin", remoteURL); err != nil {
			fmt.Fprintf(os.Stderr, "[memory] remote add origin %s: %v\n", remoteURL, err)
			return
		}
		_ = m.gitInRepo("config", "remote.origin.fetch", "+refs/heads/main:refs/remotes/origin/main")
	} else {

		if err := m.gitInRepo("remote", "set-url", "origin", remoteURL); err != nil {
			fmt.Fprintf(os.Stderr, "[memory] remote set-url origin %s: %v\n", remoteURL, err)
		}
	}
}

func (m *MemoryGitStore) isRemoteEmpty() bool {
	if config.MemoryRepoURL() == "" {
		return false
	}
	out, err := m.gitOutputInRepo("ls-remote", "origin", "HEAD")
	if err != nil {
		return false
	}
	return strings.TrimSpace(out) == ""
}

func (m *MemoryGitStore) remoteBranchExists(branch string) bool {
	if config.MemoryRepoURL() == "" {
		return false
	}
	out, err := m.gitOutputInRepo("ls-remote", "origin", "refs/heads/"+branch)
	if err != nil {
		return false
	}
	return strings.TrimSpace(out) != ""
}

func (m *MemoryGitStore) bootstrapInitialCommit() error {

	_ = m.gitInRepo("checkout", "-b", "main") // ignore error
	if err := m.gitInRepo("commit", "--allow-empty", "-m", "chore: initialise memory repository"); err != nil {
		return fmt.Errorf("initial commit: %w", err)
	}
	return nil
}

func (m *MemoryGitStore) MemoryWorktree(branch string) (*MemoryWorktree, error) {
	return m.memoryWorktreeInternal(branch, false)
}

func (m *MemoryGitStore) MemoryWorktreeLocal(branch string) (*MemoryWorktree, error) {
	return m.memoryWorktreeInternal(branch, true)
}

func (m *MemoryGitStore) memoryWorktreeInternal(branch string, skipNetwork bool) (*MemoryWorktree, error) {
	if err := m.EnsureInitialised(); err != nil {
		return nil, err
	}

	if !skipNetwork {

		m.syncRemote()

		if err := m.SelectiveFetch(branch); err != nil {
			fmt.Fprintf(os.Stderr, "[memory] selective fetch for %s: %v\n", branch, err)
		}
	}

	localExists := m.gitOutputInRepoNoErr("rev-parse", "--verify", branch) != ""

	if !localExists {

		if err := m.createOrphanBranch(branch); err != nil {
			return nil, fmt.Errorf("creating memory branch %q: %w", branch, err)
		}
	}

	wtDir := m.worktreeDirForBranch(branch)

	if _, err := os.Stat(filepath.Join(wtDir, ".git")); err == nil {
		wt := &MemoryWorktree{store: m, branch: branch, dir: wtDir}
		if !skipNetwork {
			if err := wt.Pull(); err != nil {
				fmt.Fprintf(os.Stderr, "[memory] pull %s: %v\n", branch, err)
			}
		}
		return wt, nil
	}

	if err := os.MkdirAll(m.wtBase, 0o755); err != nil {
		return nil, fmt.Errorf("creating worktrees base dir: %w", err)
	}
	if err := m.addWorktree(branch, wtDir); err != nil {
		return nil, fmt.Errorf("adding worktree for branch %q at %s: %w", branch, wtDir, err)
	}

	if err := m.RegisterBranch(branch, wtDir); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] register branch %s: %v\n", branch, err)
	}

	wt := &MemoryWorktree{store: m, branch: branch, dir: wtDir}
	if !skipNetwork {
		if err := wt.Pull(); err != nil {
			fmt.Fprintf(os.Stderr, "[memory] pull %s: %v\n", branch, err)
		}
	}

	return wt, nil
}

func (m *MemoryGitStore) worktreeDirForBranch(branch string) string {
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(branch)
	return filepath.Join(m.wtBase, safe)
}

func (m *MemoryGitStore) createOrphanBranch(branch string) error {

	emptyTree, err := m.gitOutputInRepo("hash-object", "-t", "tree", "--stdin")
	if err != nil {

		emptyTree, err = m.gitOutputInRepo("mktree")
		if err != nil {
			return fmt.Errorf("creating empty tree: %w", err)
		}
	}
	emptyTree = strings.TrimSpace(emptyTree)

	commitSHA, err := m.gitOutputInRepo("commit-tree", emptyTree, "-m", "init memory branch: "+branch)
	if err != nil {
		return fmt.Errorf("creating root commit for orphan branch: %w", err)
	}
	commitSHA = strings.TrimSpace(commitSHA)

	if err := m.gitInRepo("update-ref", "refs/heads/"+branch, commitSHA); err != nil {
		return fmt.Errorf("creating orphan branch ref: %w", err)
	}

	m.pushBranchInBackground(branch, m.worktreeDirForBranch(branch))
	return nil
}

func (m *MemoryGitStore) ExtractBranchDir(branch, relDir, destDir string) error {
	wt, err := m.MemoryWorktree(branch)
	if err != nil {
		return err
	}
	src := filepath.Join(wt.dir, relDir)
	if _, err := os.Stat(src); err != nil {
		return os.MkdirAll(destDir, 0o755)
	}
	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return err
	}
	return copyDirRecursive(src, destDir)
}

type MemoryWorktree struct {
	store  *MemoryGitStore
	branch string
	dir    string
}

func (w *MemoryWorktree) Dir() string { return w.dir }

func (w *MemoryWorktree) Pull() error {
	w.store.syncRemote()

	if config.MemoryRepoURL() == "" {
		return nil
	}

	if w.store.isRemoteEmpty() {
		return nil
	}

	if !w.store.remoteBranchExists(w.branch) {
		return nil
	}

	g := gitmod.Default()
	if err := g.Run(w.dir, "pull", "--rebase", "--autostash",
		"--allow-unrelated-histories", "-X", "ours", "--depth=1",
		"origin", w.branch); err != nil {
		return fmt.Errorf("pulling memory branch: %v", err)
	}
	return nil
}

func (w *MemoryWorktree) WriteFile(relPath string, data []byte) error {
	full := filepath.Join(w.dir, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		return err
	}
	return os.WriteFile(full, data, 0o644)
}

func (w *MemoryWorktree) ReadFile(relPath string) ([]byte, error) {
	return os.ReadFile(filepath.Join(w.dir, relPath))
}

func (w *MemoryWorktree) RemoveFile(relPath string) error {
	return os.Remove(filepath.Join(w.dir, relPath))
}

func (w *MemoryWorktree) ListDir(relDir string) ([]os.DirEntry, error) {
	return os.ReadDir(filepath.Join(w.dir, relDir))
}

func (w *MemoryWorktree) CommitAndPush(message string) error {
	wt := w.dir

	if err := gitmod.Default().Run(wt, "add", "."); err != nil {
		return fmt.Errorf("staging memory changes: %w", err)
	}

	if err := gitmod.Default().Run(wt, "diff", "--cached", "--quiet"); err == nil {
		return nil
	}

	if err := gitmod.Default().Run(wt, "commit", "-m", message); err != nil {
		return fmt.Errorf("committing memory changes: %v", err)
	}

	w.store.pushBranchInBackground(w.branch, w.dir)
	return nil
}

var pendingPushes sync.WaitGroup

func WaitForPendingPushes() { pendingPushes.Wait() }

func (m *MemoryGitStore) pushBranchInBackground(branch, wtDir string) {

	if config.MemoryRepoURL() == "" {
		return
	}

	pendingPushes.Add(1)
	go func() {
		defer pendingPushes.Done()

		m.syncRemote()

		g := gitmod.Default()

		if m.remoteBranchExists(branch) {
			if err := g.Run(wtDir, "pull", "--rebase", "--allow-unrelated-histories",
				"-X", "ours", "--depth=1", "origin", branch); err != nil {
				fmt.Fprintf(os.Stderr, "[memory] background pull --rebase %s: %v\n", branch, err)
			}
		}

		if err := g.Run(wtDir, "push", "--set-upstream", "origin", branch); err != nil {
			fmt.Fprintf(os.Stderr, "[memory] background push %s: %v\n", branch, err)
		}
	}()
}

func (w *MemoryWorktree) Prune() error {

	if _, err := w.store.DeregisterBranch(w.branch, w.dir); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] prune: deregister branch %s: %v\n", w.branch, err)
	}

	g := gitmod.Default()

	if err := g.Run(w.store.repoDir, "worktree", "remove", "--force", w.dir); err != nil {

		_ = os.RemoveAll(w.dir)
	}

	_ = g.Run(w.store.repoDir, "worktree", "prune")
	return nil
}

func (m *MemoryGitStore) git(args ...string) error {
	return gitmod.Default().RunGlobal(args...)
}

func (m *MemoryGitStore) gitInRepo(args ...string) error {
	return gitmod.Default().Run(m.repoDir, args...)
}

func (m *MemoryGitStore) gitOutputInRepo(args ...string) (string, error) {
	return gitmod.Default().RunOutput(m.repoDir, args...)
}

func (m *MemoryGitStore) gitOutputInRepoNoErr(args ...string) string {
	return gitmod.Default().RunSilent(m.repoDir, args...)
}

func (m *MemoryGitStore) addWorktree(branch, destDir string) error {
	g := gitmod.Default()
	return g.Run(m.repoDir, "worktree", "add", destDir, branch)
}

func safeMemorySymlink(source, linkPath string) error {
	return paths.SafeSymlink(source, linkPath)
}

func copyDirRecursive(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		dest := filepath.Join(dst, rel)
		if info.IsDir() {
			return os.MkdirAll(dest, info.Mode())
		}
		return copyFileData(path, dest, info.Mode())
	})
}

func copyFileData(src, dst string, mode os.FileMode) (retErr error) {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, mode)
	if err != nil {
		return err
	}
	defer func() {
		if closeErr := out.Close(); closeErr != nil && retErr == nil {
			retErr = closeErr
		}
	}()
	_, err = io.Copy(out, in)
	return err
}
