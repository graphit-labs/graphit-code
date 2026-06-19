package memory

import (
	"encoding/json"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

type memoryBranchMeta struct {
	Refs     []string `json:"refs"`
	LastUsed string   `json:"lastUsed"`
}

type memoryBranchLockFile struct {
	Version  int                          `json:"version"`
	Branches map[string]*memoryBranchMeta `json:"branches"`
}

func memoryBranchLockPath() string {
	return filepath.Join(brand.GlobalDir(), "memory.lock.json")
}

func loadMemLock() (*memoryBranchLockFile, error) {
	data, err := os.ReadFile(memoryBranchLockPath())
	if os.IsNotExist(err) {
		return &memoryBranchLockFile{Version: 1, Branches: make(map[string]*memoryBranchMeta)}, nil
	}
	if err != nil {
		return nil, err
	}
	var lf memoryBranchLockFile
	if err := json.Unmarshal(data, &lf); err != nil {
		return &memoryBranchLockFile{Version: 1, Branches: make(map[string]*memoryBranchMeta)}, nil
	}
	if lf.Branches == nil {
		lf.Branches = make(map[string]*memoryBranchMeta)
	}
	return &lf, nil
}

func saveMemLock(lf *memoryBranchLockFile) error {
	path := memoryBranchLockPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(lf, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}

func (m *MemoryGitStore) SelectiveFetch(branches ...string) error {
	if config.MemoryRepoURL() == "" {
		return nil
	}

	if len(branches) > 0 {
		needFetch := false
		for _, branch := range branches {
			remoteCommit := m.remoteBranchCommit(branch)
			if remoteCommit == "" {
				continue
			}
			localCommit := m.gitOutputInRepoNoErr("rev-parse", "--verify", branch)
			if localCommit != remoteCommit {
				needFetch = true
				break
			}
		}
		if !needFetch {
			return nil
		}
	} else if m.isRemoteEmpty() {
		return nil
	}

	if err := m.gitInRepo("fetch", "--depth=1", "--filter=blob:none", "origin"); err != nil {
		m.log().Warn("fetch failed", "error", err)
	}
	return nil
}

func (m *MemoryGitStore) activeBranches() ([]string, error) {
	lf, err := loadMemLock()
	if err != nil {
		return nil, err
	}
	var active []string
	for branch, meta := range lf.Branches {
		if len(meta.Refs) > 0 {
			active = append(active, branch)
		}
	}
	return active, nil
}

func (m *MemoryGitStore) RegisterBranch(branch, ref string) error {
	lf, err := loadMemLock()
	if err != nil {
		return err
	}
	meta := lf.Branches[branch]
	if meta == nil {
		meta = &memoryBranchMeta{}
		lf.Branches[branch] = meta
	}
	found := false
	for _, r := range meta.Refs {
		if r == ref {
			found = true
			break
		}
	}
	if !found {
		meta.Refs = append(meta.Refs, ref)
	}
	meta.LastUsed = time.Now().UTC().Format(time.RFC3339)
	return saveMemLock(lf)
}

func (m *MemoryGitStore) DeregisterBranch(branch, ref string) (bool, error) {
	lf, err := loadMemLock()
	if err != nil {
		return false, err
	}
	meta := lf.Branches[branch]
	if meta == nil {
		return false, nil
	}
	var remaining []string
	for _, r := range meta.Refs {
		if r != ref {
			remaining = append(remaining, r)
		}
	}
	meta.Refs = remaining
	unused := len(remaining) == 0
	if unused {
		delete(lf.Branches, branch)

		m.pruneLocalBranch(branch)
	}
	return unused, saveMemLock(lf)
}

func (m *MemoryGitStore) ValidateMemBranchRefs() (int, error) {
	lf, err := loadMemLock()
	if err != nil {
		return 0, err
	}
	cleaned := 0
	for branch, meta := range lf.Branches {
		var alive []string
		for _, ref := range meta.Refs {
			if ref == "user" {
				alive = append(alive, ref)
				continue
			}

			lockFile := filepath.Join(ref, brand.LockFileName())
			if _, err := os.Stat(lockFile); err == nil {
				alive = append(alive, ref)
			} else {
				cleaned++
			}
		}
		meta.Refs = alive
		if len(alive) == 0 {
			delete(lf.Branches, branch)
			m.pruneLocalBranch(branch)
		}
	}
	if cleaned > 0 {
		return cleaned, saveMemLock(lf)
	}
	return 0, nil
}

func (m *MemoryGitStore) pruneLocalBranch(branch string) {
	wtDir := m.worktreeDirForBranch(branch)
	if err := os.RemoveAll(wtDir); err != nil {
		m.log().Warn("prune: remove worktree failed", "dir", wtDir, "error", err)
	}
	if err := m.gitInRepo("branch", "-D", branch); err != nil {
		m.log().Warn("prune: delete branch failed", "branch", branch, "error", err)
	}
}

func (m *MemoryGitStore) ActiveMemoryBranches() ([]string, error) {
	return m.activeBranches()
}

func (m *MemoryGitStore) MemoryBranchSummary() (map[string][]string, error) {
	lf, err := loadMemLock()
	if err != nil {
		return nil, err
	}
	out := make(map[string][]string, len(lf.Branches))
	for branch, meta := range lf.Branches {
		out[branch] = meta.Refs
	}
	return out, nil
}
