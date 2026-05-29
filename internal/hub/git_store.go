package hub

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type GitStore struct {
	Logger    *slog.Logger
	repoDir   string
	cacheBase string
}

func (g *GitStore) log() *slog.Logger { return slogutil.Resolve(g.Logger) }

func NewGitStore(inlineCfg, projectCfg config.ConfigMap) (*GitStore, error) {
	hubDir, err := config.HubRepoDirPath()
	if err != nil {
		return nil, fmt.Errorf("resolving hub directory: %w", err)
	}
	return &GitStore{repoDir: hubDir, cacheBase: hubDir + "-cache"}, nil
}

func (g *GitStore) Dir() string {
	return g.repoDir
}

func (g *GitStore) CacheBase() string {
	return g.cacheBase
}

func (g *GitStore) EnsureInitialised() error {
	if err := os.MkdirAll(g.repoDir, 0o755); err != nil {
		return fmt.Errorf("creating hub repo dir: %w", err)
	}

	if _, err := os.Stat(filepath.Join(g.repoDir, ".git")); os.IsNotExist(err) {
		if err := g.git("init", "--initial-branch=main", g.repoDir); err != nil {
			return fmt.Errorf("initialising hub git repo: %w", err)
		}

		if err := g.bootstrapEmptyRepo(); err != nil {
			return fmt.Errorf("bootstrapping hub repo: %w", err)
		}
	}

	if err := os.MkdirAll(g.cacheBase, 0o755); err != nil {
		return fmt.Errorf("creating hub-cache dir: %w", err)
	}

	if err := g.gitInRepo("config", "fetch.depth", "1"); err != nil {
		g.log().Warn("git config fetch.depth", "error", err)
	}

	if err := g.gitInRepo("config", "remote.origin.fetch", "+refs/heads/main:refs/remotes/origin/main"); err != nil {
		g.log().Warn("git config remote.origin.fetch", "error", err)
	}

	if out, err := g.gitOutputInRepo("for-each-ref", "--format=%(refname)", "refs/remotes/origin/"); err == nil {
		for _, ref := range strings.Split(out, "\n") {
			ref = strings.TrimSpace(ref)
			if ref != "" && ref != "refs/remotes/origin/main" {
				if err := g.gitInRepo("update-ref", "-d", ref); err != nil {
					g.log().Warn("prune stale ref", "ref", ref, "error", err)
				}
			}
		}
	}

	g.syncRemote()

	return nil
}

func (g *GitStore) syncRemote() {
	remoteURL := config.HubRepoURL()
	if remoteURL == "" {

		_ = g.gitInRepo("remote", "remove", "origin") // ignore error
		return
	}

	out, _ := g.gitOutputInRepo("remote", "get-url", "origin")
	if strings.TrimSpace(out) == remoteURL {
		return
	}

	if strings.TrimSpace(out) == "" {

		if err := g.gitInRepo("remote", "add", "origin", remoteURL); err != nil {
			g.log().Warn("remote add origin", "url", remoteURL, "error", err)
		}
	} else {

		if err := g.gitInRepo("remote", "set-url", "origin", remoteURL); err != nil {
			g.log().Warn("remote set-url origin", "url", remoteURL, "error", err)
		}
	}
}

func (g *GitStore) hasRemote() bool {
	return config.HubRepoURL() != ""
}

func (g *GitStore) EnsureCloned() error {
	if err := g.EnsureInitialised(); err != nil {
		return err
	}
	if g.hasRemote() {
		if err := g.Sync(); err != nil {
			return err
		}
	}

	_ = g.EnsureEventsClone()
	return nil
}

func (g *GitStore) Sync() error {
	if !g.hasRemote() {
		return nil
	}

	g.syncRemote()

	if g.isEmptyRepo() || g.isRemoteEmpty() {
		return nil
	}

	branch := g.currentBranch()
	if branch == "" {
		branch = "main"
	}

	err := g.gitInRepo("pull", "--rebase", "--autostash", "--depth=1", "origin", branch)
	if err != nil {

		if g.isRebasing() {
			if abortErr := g.gitInRepo("rebase", "--abort"); abortErr != nil {
				g.log().Error("rebase --abort failed", "error", abortErr)
			}
			return fmt.Errorf("conflict detected during hub sync — another user may have modified the same artifact version; please retry")
		}
		return fmt.Errorf("syncing hub repository: %w", err)
	}
	return nil
}

func (g *GitStore) CommitAndPush(message string) error {

	if err := g.gitInRepo("add", "-A"); err != nil {
		return fmt.Errorf("staging changes: %w", err)
	}

	if err := g.gitInRepo("diff", "--cached", "--quiet"); err == nil {
		return nil
	}

	if err := g.gitInRepo("commit", "-m", message); err != nil {
		return fmt.Errorf("committing changes: %w", err)
	}

	if !g.hasRemote() {
		return nil
	}

	g.syncRemote()

	if err := g.Sync(); err != nil {
		return err
	}

	if err := g.gitInRepo("push", "--set-upstream", "origin", "HEAD"); err != nil {
		return fmt.Errorf("pushing to remote: %w", err)
	}

	return nil
}

func (g *GitStore) ReadFile(relPath string) ([]byte, error) {
	return os.ReadFile(filepath.Join(g.repoDir, relPath))
}

func (g *GitStore) WriteFile(relPath string, data []byte) error {
	fullPath := filepath.Join(g.repoDir, relPath)
	if err := os.MkdirAll(filepath.Dir(fullPath), 0o755); err != nil {
		return err
	}
	return os.WriteFile(fullPath, data, 0o644)
}

func (g *GitStore) RemoveAll(relPath string) error {
	return os.RemoveAll(filepath.Join(g.repoDir, relPath))
}

func (g *GitStore) AbsPath(relPath string) string {
	return filepath.Join(g.repoDir, relPath)
}

func (g *GitStore) HeadCommit() string {
	out, err := g.gitOutputInRepo("rev-parse", "HEAD")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}

func gitExec() gitmod.Git { return gitmod.Default() }

func (g *GitStore) git(args ...string) error {
	return gitExec().RunGlobal(args...)
}

func (g *GitStore) gitInRepo(args ...string) error {
	return gitExec().Run(g.repoDir, args...)
}

func (g *GitStore) gitOutputInRepo(args ...string) (string, error) {
	return gitExec().RunOutput(g.repoDir, args...)
}

func (g *GitStore) isRebasing() bool {
	rebaseMerge := filepath.Join(g.repoDir, ".git", "rebase-merge")
	rebaseApply := filepath.Join(g.repoDir, ".git", "rebase-apply")
	_, err1 := os.Stat(rebaseMerge)
	_, err2 := os.Stat(rebaseApply)
	return err1 == nil || err2 == nil
}

func (g *GitStore) currentBranch() string {
	out, err := g.gitOutputInRepo("rev-parse", "--abbrev-ref", "HEAD")
	if err != nil {
		return ""
	}
	branch := strings.TrimSpace(out)
	if branch == "HEAD" {
		return ""
	}
	return branch
}

func (g *GitStore) gitOutputInRepoNoErr(args ...string) string {
	out, err := g.gitOutputInRepo(args...)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}

func (g *GitStore) isEmptyRepo() bool {
	_, err := g.gitOutputInRepo("rev-parse", "--verify", "HEAD")
	return err != nil
}

func (g *GitStore) isRemoteEmpty() bool {
	if !g.hasRemote() {
		return false
	}
	out, err := g.gitOutputInRepo("ls-remote", "origin", "HEAD")
	if err != nil {
		return false
	}
	return strings.TrimSpace(out) == ""
}

func (g *GitStore) bootstrapEmptyRepo() error {

	globalDir := projectDir("")

	seed := map[string]string{
		"baselines.json":            `{"baselines":[]}`,
		globalDir + "/project.json": `{"v":1}`,
	}
	for name, content := range seed {
		path := filepath.Join(g.repoDir, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return fmt.Errorf("creating dir for %s: %w", name, err)
		}
		if err := os.WriteFile(path, []byte(content+"\n"), 0o644); err != nil {
			return fmt.Errorf("writing %s: %w", name, err)
		}
	}

	if err := g.gitInRepo("add", "-A"); err != nil {
		return fmt.Errorf("staging initial structure: %w", err)
	}
	if err := g.gitInRepo("commit", "-m", "chore: initialise hub repository"); err != nil {
		return fmt.Errorf("creating initial commit: %w", err)
	}

	if g.hasRemote() {
		branch := g.currentBranch()
		if branch == "" {
			branch = "main"
		}
		_ = g.gitInRepo("push", "--set-upstream", "origin", branch) //nolint:errcheck // best-effort initial push
	}

	return nil
}

func eventsRefPrefix() string {
	secret := getOrCreateClientSecret()
	clientID := getOrCreateClientID()
	if secret == "" && clientID == "" {
		return "refs/events/_default/"
	}
	h := sha256.Sum256([]byte(secret + clientID))
	return "refs/events/" + hex.EncodeToString(h[:16]) + "/"
}

func (g *GitStore) eventsStagingDir() string {
	return filepath.Join(g.cacheBase, "events-staging")
}

func (g *GitStore) WriteEventFile(key string, data []byte) {
	stagingDir := g.eventsStagingDir()
	targetPath := filepath.Join(stagingDir, key)
	if err := os.MkdirAll(filepath.Dir(targetPath), 0755); err != nil {
		return
	}
	_ = os.WriteFile(targetPath, data, 0644)
}

func (g *GitStore) EnsureEventsClone() error {
	return os.MkdirAll(g.eventsStagingDir(), 0o755)
}

func (g *GitStore) SyncEvents() {
	stagingDir := g.eventsStagingDir()
	if err := os.MkdirAll(stagingDir, 0o755); err != nil {
		return
	}

	remoteURL := config.HubRepoURL()
	if remoteURL == "" {
		return
	}

	entries, err := os.ReadDir(stagingDir)
	if err != nil || len(entries) == 0 {
		return
	}

	gt := gitExec()
	treeHash, err := g.buildTreeFromDir(stagingDir)
	if err != nil {
		g.log().Warn("events: build tree", "error", err)
		return
	}

	commitSHA, err := g.gitOutputInRepo("commit-tree", treeHash, "-m", "events batch: "+generateULID())
	if err != nil {
		g.log().Warn("events: commit-tree", "error", err)
		return
	}
	commitSHA = strings.TrimSpace(commitSHA)

	refName := eventsRefPrefix() + generateULID()
	if err := g.gitInRepo("update-ref", refName, commitSHA); err != nil {
		g.log().Warn("events: update-ref", "ref", refName, "error", err)
		return
	}

	if err := gt.Run(g.repoDir, "push", "origin", refName); err != nil {
		g.log().Warn("events: push", "ref", refName, "error", err)
		return
	}

	_ = os.RemoveAll(stagingDir)
	_ = os.MkdirAll(stagingDir, 0o755)
	_ = g.gitInRepo("update-ref", "-d", refName)
}

func (g *GitStore) buildTreeFromDir(dir string) (string, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return "", err
	}

	var treeEntries []string
	for _, e := range entries {
		fullPath := filepath.Join(dir, e.Name())
		if e.IsDir() {
			subtreeHash, err := g.buildTreeFromDir(fullPath)
			if err != nil {
				return "", err
			}
			treeEntries = append(treeEntries, fmt.Sprintf("040000 tree %s\t%s", subtreeHash, e.Name()))
		} else {
			blobHash, err := g.gitOutputInRepo("hash-object", "-w", fullPath)
			if err != nil {
				return "", fmt.Errorf("hash-object %s: %w", fullPath, err)
			}
			treeEntries = append(treeEntries, fmt.Sprintf("100644 blob %s\t%s", strings.TrimSpace(blobHash), e.Name()))
		}
	}

	if len(treeEntries) == 0 {
		return "", fmt.Errorf("empty directory: %s", dir)
	}

	input := strings.Join(treeEntries, "\n") + "\n"
	cmd := exec.Command("git", "mktree")
	cmd.Dir = g.repoDir
	cmd.Stdin = bytes.NewReader([]byte(input))
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("mktree: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

func ArtifactBranchName(artType ArtifactType, id, version, projectID string) string {
	folder := TypeFolderMap[artType]
	if folder == "" {
		folder = string(artType)
	}
	projSegment := projectID
	if projSegment == "" {
		projSegment = "_global"
	}

	if artType == TypeAST || artType == TypeKnowledge {
		return fmt.Sprintf("artifact/%s/%s/%s", folder, projSegment, version)
	}
	return fmt.Sprintf("artifact/%s/%s/%s/%s", folder, projSegment, id, version)
}

func (g *GitStore) ArtifactCloneDir(artType ArtifactType, id, version, projectID string) string {
	folder := TypeFolderMap[artType]
	if folder == "" {
		folder = string(artType)
	}
	projSegment := projectID
	if projSegment == "" {
		projSegment = "_global"
	}

	if artType == TypeAST || artType == TypeKnowledge {
		return filepath.Join(g.cacheBase, folder, projSegment, version)
	}
	return filepath.Join(g.cacheBase, folder, projSegment, id, version)
}

func (g *GitStore) EnsureArtifactClone(artType ArtifactType, id, version, projectID string) (string, error) {
	branch := ArtifactBranchName(artType, id, version, projectID)
	destDir := g.ArtifactCloneDir(artType, id, version, projectID)

	if entries, err := os.ReadDir(destDir); err == nil && len(entries) > 0 {
		if g.hasRemote() {

			directRefspec := fmt.Sprintf("+refs/heads/%s:refs/heads/%s", branch, branch)
			if gitExec().Run(g.repoDir, "fetch", "--depth=1", "origin", directRefspec) == nil {

				_ = g.extractBranchToDir(branch, destDir)
			}
		}
		return destDir, nil
	}

	if err := os.MkdirAll(filepath.Dir(destDir), 0o755); err != nil {
		return "", fmt.Errorf("creating artifact cache parent dir: %w", err)
	}

	branchExists := false
	if g.hasRemote() {
		directRefspec := fmt.Sprintf("+refs/heads/%s:refs/heads/%s", branch, branch)
		if err := gitExec().Run(g.repoDir, "fetch", "--depth=1", "origin", directRefspec); err == nil {
			if g.gitOutputInRepoNoErr("rev-parse", "--verify", "refs/heads/"+branch) != "" {
				branchExists = true
			}
		}
	} else {

		if g.gitOutputInRepoNoErr("rev-parse", "--verify", "refs/heads/"+branch) != "" {
			branchExists = true
		}
	}

	if !branchExists {

		if err := g.createOrphanBranchInRepo(branch); err != nil {
			return "", fmt.Errorf("creating orphan branch %q: %w", branch, err)
		}
	}

	if err := g.extractBranchToDir(branch, destDir); err != nil {
		return "", fmt.Errorf("extracting artifact archive for %q: %w", branch, err)
	}

	return destDir, nil
}

func (g *GitStore) extractBranchToDir(branch, destDir string) error {

	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return fmt.Errorf("creating dest dir: %w", err)
	}

	entries, _ := os.ReadDir(destDir)
	for _, e := range entries {
		_ = os.RemoveAll(filepath.Join(destDir, e.Name()))
	}

	archiveCmd := exec.Command("git", "archive", branch)
	archiveCmd.Dir = g.repoDir

	tarCmd := exec.Command("tar", "-x", "-C", destDir)

	pipe, err := archiveCmd.StdoutPipe()
	if err != nil {
		return fmt.Errorf("creating pipe: %w", err)
	}
	tarCmd.Stdin = pipe

	var archiveStderr, tarStderr bytes.Buffer
	archiveCmd.Stderr = &archiveStderr
	tarCmd.Stderr = &tarStderr

	if err := archiveCmd.Start(); err != nil {
		return fmt.Errorf("starting git archive: %w", err)
	}
	if err := tarCmd.Start(); err != nil {
		_ = archiveCmd.Wait()
		return fmt.Errorf("starting tar: %w", err)
	}

	archiveErr := archiveCmd.Wait()
	tarErr := tarCmd.Wait()

	if archiveErr != nil {
		return fmt.Errorf("git archive: %w (stderr: %s)", archiveErr, archiveStderr.String())
	}
	if tarErr != nil {
		return fmt.Errorf("tar extract: %w (stderr: %s)", tarErr, tarStderr.String())
	}

	return nil
}

func (g *GitStore) WriteArtifactBranch(artType ArtifactType, id, version, projectID, srcDir string) error {
	branch := ArtifactBranchName(artType, id, version, projectID)

	treeHash, err := g.buildTreeFromDir(srcDir)
	if err != nil {
		return fmt.Errorf("building tree from %s: %w", srcDir, err)
	}

	existingCommit := g.gitOutputInRepoNoErr("rev-parse", "--verify", "refs/heads/"+branch)
	if existingCommit != "" {
		existingTree := g.gitOutputInRepoNoErr("rev-parse", existingCommit+"^{tree}")
		if existingTree == treeHash {
			return nil
		}
	}

	commitMsg := fmt.Sprintf("publish %s@%s", id, version)
	var commitSHA string
	if existingCommit != "" {
		out, err := g.gitOutputInRepo("commit-tree", treeHash, "-p", existingCommit, "-m", commitMsg)
		if err != nil {
			return fmt.Errorf("commit-tree (child): %w", err)
		}
		commitSHA = strings.TrimSpace(out)
	} else {
		out, err := g.gitOutputInRepo("commit-tree", treeHash, "-m", commitMsg)
		if err != nil {
			return fmt.Errorf("commit-tree (orphan): %w", err)
		}
		commitSHA = strings.TrimSpace(out)
	}

	if err := g.gitInRepo("update-ref", "refs/heads/"+branch, commitSHA); err != nil {
		return fmt.Errorf("update-ref %s: %w", branch, err)
	}

	if g.hasRemote() {
		if err := gitExec().Run(g.repoDir, "push", "--force", "origin", branch); err != nil {
			return fmt.Errorf("pushing artifact branch %s: %w", branch, err)
		}
	}

	destDir := g.ArtifactCloneDir(artType, id, version, projectID)
	_ = g.extractBranchToDir(branch, destDir)

	return nil
}

func (g *GitStore) DeleteArtifactBranch(artType ArtifactType, id, version, projectID string) error {
	branch := ArtifactBranchName(artType, id, version, projectID)
	cacheDir := g.ArtifactCloneDir(artType, id, version, projectID)

	if g.hasRemote() {
		_ = gitExec().Run(g.repoDir, "push", "origin", "--delete", branch)
	}

	_ = os.RemoveAll(cacheDir)

	_ = g.gitInRepo("branch", "-D", branch)

	return nil
}

type MemoryWorktree struct {
	g      *GitStore
	branch string
	dir    string
}

func (g *GitStore) worktreesBaseDir() string {
	return g.cacheBase
}

func (g *GitStore) worktreeDirForBranch(branch string) string {
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(branch)
	return filepath.Join(g.worktreesBaseDir(), safe)
}

func (g *GitStore) MemoryWorktree(branch string) (*MemoryWorktree, error) {

	if err := g.EnsureInitialised(); err != nil {
		return nil, err
	}

	if g.hasRemote() {
		g.syncRemote()
		directRefspec := fmt.Sprintf("+refs/heads/%s:refs/heads/%s", branch, branch)
		if err := g.gitInRepo("fetch", "--depth=1", "--filter=blob:none", "origin", directRefspec); err != nil {
			g.log().Warn("memory fetch", "branch", branch, "error", err)
		}
	}

	localExists := g.gitOutputInRepoNoErr("rev-parse", "--verify", branch) != ""

	if !localExists {

		if err := g.createOrphanBranch(branch); err != nil {
			return nil, fmt.Errorf("creating memory branch %q: %w", branch, err)
		}
	}

	wtDir := g.worktreeDirForBranch(branch)

	if _, err := os.Stat(filepath.Join(wtDir, ".git")); err == nil {
		wt := &MemoryWorktree{g: g, branch: branch, dir: wtDir}
		if g.hasRemote() {
			if err := wt.Pull(); err != nil {
				g.log().Warn("memory refresh", "branch", branch, "error", err)
			}
		}
		return wt, nil
	}

	if err := os.MkdirAll(g.worktreesBaseDir(), 0o755); err != nil {
		return nil, fmt.Errorf("creating worktrees base dir: %w", err)
	}

	if err := g.addWorktree(branch, wtDir); err != nil {
		return nil, fmt.Errorf("adding worktree for branch %q at %s: %w", branch, wtDir, err)
	}

	return &MemoryWorktree{g: g, branch: branch, dir: wtDir}, nil
}

func (w *MemoryWorktree) Dir() string { return w.dir }

func (w *MemoryWorktree) Pull() error {
	if !w.g.hasRemote() {
		return nil
	}
	w.g.syncRemote()

	out := w.g.gitOutputInRepoNoErr("ls-remote", "origin", "refs/heads/"+w.branch)
	if strings.TrimSpace(out) == "" {
		return nil
	}

	if err := gitExec().Run(w.dir, "pull", "--rebase", "--autostash", "--depth=1",
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
	g := gitExec()

	if err := g.Run(w.dir, "add", "-A"); err != nil {
		return fmt.Errorf("staging memory changes: %w", err)
	}

	if err := g.Run(w.dir, "diff", "--cached", "--quiet"); err == nil {
		return nil
	}

	if err := g.Run(w.dir, "commit", "-m", message); err != nil {
		return fmt.Errorf("committing memory changes: %v", err)
	}

	if !w.g.hasRemote() {
		return nil
	}
	w.g.syncRemote()

	if err := g.Run(w.dir, "pull", "--rebase", "-X", "ours", "--depth=1", "origin", w.branch); err != nil {
		w.g.log().Warn("pre-push rebase", "branch", w.branch, "error", err)
	}

	if err := g.Run(w.dir, "push", "--set-upstream", "origin", w.branch); err != nil {
		return fmt.Errorf("pushing memory branch %q: %v", w.branch, err)
	}

	return nil
}

func (w *MemoryWorktree) Prune() error {
	g := gitExec()
	if err := g.Run(w.g.repoDir, "worktree", "remove", "--force", w.dir); err != nil {
		_ = os.RemoveAll(w.dir)
	}
	_ = g.Run(w.g.repoDir, "worktree", "prune")
	return nil
}

func (g *GitStore) ExtractBranchDir(branch, relDir, destDir string) error {
	wt, err := g.MemoryWorktree(branch)
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
	return copyDir(src, destDir)
}

func (g *GitStore) createOrphanBranch(branch string) error {

	emptyTree, err := g.gitOutputInRepo("hash-object", "-t", "tree", "--stdin")
	if err != nil {

		emptyTree, err = g.gitOutputInRepo("mktree")
		if err != nil {
			return fmt.Errorf("creating empty tree: %w", err)
		}
	}
	emptyTree = strings.TrimSpace(emptyTree)

	commitSHA, err := g.gitOutputInRepo("commit-tree", emptyTree, "-m", "init branch: "+branch)
	if err != nil {
		return fmt.Errorf("creating root commit for orphan branch: %w", err)
	}
	commitSHA = strings.TrimSpace(commitSHA)

	if err := g.gitInRepo("update-ref", "refs/heads/"+branch, commitSHA); err != nil {
		return fmt.Errorf("creating orphan branch ref: %w", err)
	}

	return nil
}

func (g *GitStore) createOrphanBranchInRepo(branch string) error {

	emptyTree, err := g.gitOutputInRepo("hash-object", "-t", "tree", "--stdin")
	if err != nil {
		emptyTree, err = g.gitOutputInRepo("mktree")
		if err != nil {
			return fmt.Errorf("creating empty tree: %w", err)
		}
	}
	emptyTree = strings.TrimSpace(emptyTree)

	commitSHA, err := g.gitOutputInRepo("commit-tree", emptyTree, "-m", "init branch: "+branch)
	if err != nil {
		return fmt.Errorf("creating root commit: %w", err)
	}
	commitSHA = strings.TrimSpace(commitSHA)

	return g.gitInRepo("update-ref", "refs/heads/"+branch, commitSHA)
}

func (g *GitStore) addWorktree(branch, destDir string) error {
	return gitExec().Run(g.repoDir, "worktree", "add", destDir, branch)
}
