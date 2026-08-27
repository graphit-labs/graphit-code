package memory

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

// Identity and history addressing, without git.
//
// The user scope used to be keyed by `git config user.email`, and the project root was found with
// `git rev-parse --show-toplevel`. Both are gone with the rest of git.
//
// The identity itself is NOT here: it is config.UnitID, in the global configuration, because
// "which installation is this" is not a memory concept. This file only derives a scope id from it.

// UserScopeID is the id of the `user` memory scope.
//
// It hashes the unit identity rather than using it directly, because `unit.id` may be set to
// anything a person finds memorable — an email, a name — and that value goes into a directory name
// and an object key. Hashing normalises it to a token that is safe in both, and keeps the same
// 16-hex-character shape the git-email hash produced.
func UserScopeID() (string, error) {
	unit, err := config.UnitID()
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256([]byte(unit))
	return fmt.Sprintf("%x", sum)[:16], nil
}

// unitOrEmpty is the identity for a frontmatter field: a write must not fail because the identity
// could not be resolved, so an unresolvable unit records nothing rather than erroring.
func unitOrEmpty() string {
	id, err := config.UnitID()
	if err != nil {
		return ""
	}
	return id
}

// HistoryDirName holds the archived revisions of a scope's memories.
//
// It is a subdirectory, which is what keeps it out of everything: every listing in this package
// reads one level with os.ReadDir and skips directories, so an archived revision is never indexed
// as a memory, never compiled into the wiki, and never listed. Changing any of those listings to
// WalkDir would turn every archived revision into a duplicate memory.
const HistoryDirName = "history"

// HistoryPath is where one revision of a memory is archived, relative to the scope directory.
//
// Zero-padded so a listing sorts chronologically, and keyed by the memory id rather than by its
// file name — the file name carries importance and can change, the id cannot.
func HistoryPath(id string, revision int) string {
	if revision < 1 {
		revision = 1
	}
	return path.Join(HistoryDirName, id, fmt.Sprintf("%04d.md", revision))
}

// projectRootDir finds the project the working directory belongs to.
//
// A project is identified by its lockfile, which is the framework's own marker — the same one
// store.ProjectID reads. Walking up for it replaces `git rev-parse --show-toplevel`, and is
// strictly more correct here: a project without git still has a lockfile, and a git repository
// holding several projects resolves to the right one instead of to the repository root.
func projectRootDir() string {
	wd, err := os.Getwd()
	if err != nil {
		return ""
	}
	lock := brand.LockFileName()
	dir := wd
	for {
		if _, err := os.Stat(filepath.Join(dir, lock)); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return wd
		}
		dir = parent
	}
}
