package memory

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"strings"

	"github.com/oklog/ulid/v2"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

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
// It is a subdirectory, and that is still what keeps it out of every LISTING: each listing in
// this package reads one level with os.ReadDir and skips directories, so an archived revision
// is not part of the catalogue of what this project knows. It IS compiled into the wiki and it
// IS searchable — see collectHistoryDocs, which walks it deliberately rather than by widening
// one of those listings.
const HistoryDirName = "history"

// HistoryPath is where one revision of a memory is archived, relative to the scope directory.
//
// Keyed by the memory id, which is the chain identity and never changes, then by the revision's
// own identifier. New revisions are named by ULID: lexicographically time-ordered, so a listing
// still sorts chronologically, and collision-free, so two units archiving concurrently cannot
// overwrite each other — which a shared counter could.
//
// Archives written before this carry a zero-padded counter (`0001.md`). They are not renamed: a
// rename would invalidate the `previous` pointer of a live memory. Both forms coexist, and the
// ordering survives because "0001" precedes every ULID lexicographically.
func HistoryPath(id, revisionID string) string {
	if revisionID == "" {
		revisionID = NewRevisionID()
	}
	return path.Join(HistoryDirName, id, revisionID+".md")
}

// HistoryDirFor is the directory holding one memory's archived revisions.
func HistoryDirFor(id string) string {
	return path.Join(HistoryDirName, id)
}

// NewRevisionID mints the address of one archived revision.
func NewRevisionID() string {
	return ulid.Make().String()
}

// RevisionIDFromHistoryPath recovers a revision's own identifier from its archive path.
func RevisionIDFromHistoryPath(rel string) string {
	if rel == "" {
		return ""
	}
	return strings.TrimSuffix(path.Base(filepath.ToSlash(rel)), ".md")
}

func MemoryIDFor(content, name string) string {
	if id := ParseMemoryFrontmatter(content).ID; id != "" {
		return id
	}
	return MemoryIDFromFileName(name)
}

func IsMemoryID(s string) bool {
	if len(s) != 26 {
		return false
	}
	for _, r := range s {
		if !strings.ContainsRune("0123456789ABCDEFGHJKMNPQRSTVWXYZ", r) {
			return false
		}
	}
	return true
}

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
