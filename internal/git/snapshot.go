package git

import (
	"fmt"
	"os"
	"strings"
)

// Snapshot identifies the checked-out Git state that owns a generated artifact.
type Snapshot struct {
	Root      string
	Branch    string
	Commit    string
	Ancestors []string
	Dirty     bool
	Detached  bool
}

// InspectSnapshot resolves the current repository, branch, commit, ancestry, and dirty state.
func InspectSnapshot(repoDir string) (Snapshot, error) {
	backend, err := DefaultErr()
	if err != nil {
		return Snapshot{}, err
	}
	return inspectSnapshot(backend, repoDir)
}

func inspectSnapshot(backend Git, repoDir string) (Snapshot, error) {
	root, err := backend.RunOutput(repoDir, "rev-parse", "--show-toplevel")
	if err != nil {
		return Snapshot{}, fmt.Errorf("not a Git repository: %w", err)
	}
	commit, err := backend.RunOutput(repoDir, "rev-parse", "HEAD")
	if err != nil {
		return Snapshot{}, fmt.Errorf("resolve Git HEAD: %w", err)
	}
	branch, err := backend.RunOutput(repoDir, "symbolic-ref", "--quiet", "--short", "HEAD")
	detached := err != nil
	if err != nil {
		branch = strings.TrimPrefix(strings.TrimSpace(os.Getenv("GRAPHIT_GIT_BASE_BRANCH")), "refs/heads/")
	}
	status, err := backend.RunOutput(repoDir, "status", "--porcelain", "--untracked-files=normal")
	if err != nil {
		return Snapshot{}, fmt.Errorf("read Git worktree state: %w", err)
	}
	ancestry, err := backend.RunOutput(repoDir, "rev-list", "HEAD")
	if err != nil {
		return Snapshot{}, fmt.Errorf("read Git ancestry: %w", err)
	}
	return Snapshot{
		Root:      strings.TrimSpace(root),
		Branch:    strings.TrimSpace(branch),
		Commit:    strings.TrimSpace(commit),
		Ancestors: strings.Fields(ancestry),
		Dirty:     strings.TrimSpace(status) != "",
		Detached:  detached,
	}, nil
}

// BranchVersion returns the mutable Hub version for this branch.
func (s Snapshot) BranchVersion() string {
	if s.Branch == "" {
		return ""
	}
	return "branch/" + s.Branch
}
