package projectversion

import (
	"context"
	"encoding/hex"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
)

// Checkout is the version checked out in projectDir itself, ignoring every override:
// what Current answers outside any scoped call and without an environment pin.
func Checkout(projectDir string) Info { return inspect(projectDir) }

// Worktree is one checkout of a repository and the version checked out in it.
type Worktree struct {
	Dir      string
	Version  string
	Detached bool
}

// Worktrees lists every checkout of the repository projectDir belongs to — the main
// one and each linked worktree that still exists — with the version each has checked
// out. It reads Git's own files, as Resolve does. A directory outside Git has none.
func Worktrees(projectDir string) []Worktree {
	repo, ok := findRepository(projectDir)
	if !ok {
		return nil
	}
	var dirs []string
	if filepath.Base(repo.commonDir) == ".git" {
		dirs = append(dirs, filepath.Dir(repo.commonDir))
	}
	entries, _ := os.ReadDir(filepath.Join(repo.commonDir, "worktrees"))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		data, err := os.ReadFile(filepath.Join(repo.commonDir, "worktrees", entry.Name(), "gitdir"))
		if err != nil {
			continue
		}
		dotGit := strings.TrimSpace(string(data))
		if dotGit == "" || !exists(dotGit) {
			continue
		}
		dirs = append(dirs, filepath.Dir(filepath.Clean(dotGit)))
	}
	out := make([]Worktree, 0, len(dirs))
	seen := map[string]bool{}
	for _, dir := range dirs {
		if seen[dir] {
			continue
		}
		seen[dir] = true
		info := inspect(dir)
		out = append(out, Worktree{Dir: dir, Version: info.Version, Detached: info.Detached})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Dir < out[j].Dir })
	return out
}

// BranchExists reports whether the repository projectDir belongs to still has a local
// branch named version.
func BranchExists(projectDir, version string) bool {
	repo, ok := findRepository(projectDir)
	if !ok || version == "" {
		return false
	}
	return repo.refExists("refs/heads/" + version)
}

// VersionFromLanceBranch reverses LanceBranchName for a non-default version: a name
// Lance could not hold is stored as "h-" plus its hexadecimal bytes.
func VersionFromLanceBranch(branch string) string {
	if encoded, ok := strings.CutPrefix(branch, "h-"); ok {
		if decoded, err := hex.DecodeString(encoded); err == nil && len(decoded) > 0 {
			return string(decoded)
		}
	}
	return branch
}

// A project opened by ID alone — a Hub project with no checkout on this machine — has
// no directory to resolve a version from. AcquireRemote scopes the running call to an
// explicit version of it, with the same exclusivity Acquire gives a directory.
func remoteKey(projectID string) string {
	if projectID = strings.TrimSpace(projectID); projectID == "" {
		return ""
	}
	return "project-id:" + projectID
}

// AcquireRemote scopes the running call to version of the project with projectID
// until release is called. An empty version is the project's default version.
//
// confirmed says the version is known to be a real one — it already has a native Lance
// branch in one of the project's stores — so a store that lacks the branch may create
// it. An unconfirmed version could be the default version under its own name, which
// has no branch; its stores read the main lineage and refuse writes.
func AcquireRemote(ctx context.Context, projectID, version string, confirmed bool) (release func(), err error) {
	version, err = Validate(version)
	if err != nil {
		return nil, err
	}
	key := remoteKey(projectID)
	unlock, err := acquireKey(ctx, key, version)
	if err != nil || key == "" || version == "" {
		return unlock, err
	}
	if confirmed {
		remoteConfirmed.Store(key, true)
	}
	return func() {
		remoteConfirmed.Delete(key)
		unlock()
	}, nil
}

var remoteConfirmed sync.Map // remote key -> true

// RemoteForkable reports whether stores of the project with projectID may create the
// branch of the version the running call selected: true when it is confirmed, or when
// no version is selected (the main lineage needs no branch).
func RemoteForkable(projectID string) bool {
	if RemoteVersion(projectID) == "" {
		return true
	}
	_, ok := remoteConfirmed.Load(remoteKey(projectID))
	return ok
}

// RemoteVersion is the version the running call selected for the project with
// projectID, or "" for its default version.
func RemoteVersion(projectID string) string {
	if v, ok := overrides.Load(remoteKey(projectID)); ok {
		return v.(string)
	}
	return ""
}

// RemoteLanceBranch is the native Lance branch of the version selected for the project
// with projectID: none for its default version. Without a checkout the default
// version's name is unknown, so the default is only ever selected by omission.
func RemoteLanceBranch(projectID string) string {
	return LanceBranchName(RemoteVersion(projectID), "")
}
