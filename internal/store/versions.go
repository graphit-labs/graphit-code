package store

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

// versionsNamespace holds one store directory per project version beneath a project's
// own store root, so `ast/project/<id>/versions/<version>/` and
// `wiki/knowledge/project/<id>/versions/<version>/`.
//
// Every branch and worktree of a repository shares the lockfile ULID, but not its code,
// so a project's derived stores are kept per version. See internal/projectversion for
// how the version of a checkout is resolved.
const versionsNamespace = "versions"

// forkLockWait bounds how long creating a version waits for a writer of the default
// version to finish before giving up on inheriting it and indexing from scratch.
const forkLockWait = 2 * time.Minute

// VersionStoreDir is the directory of one version inside a project's store root.
func VersionStoreDir(root, version string) string {
	return filepath.Join(root, versionsNamespace, VersionPathSegment(version))
}

// projectVersionDir resolves the store of projectDir's current version under root,
// first moving a store written before versions existed into the default version.
//
// The move is a rename, so it is safe on the read path; creating a version that does
// not exist yet is not, and is left to ensureProjectVersionDir.
func projectVersionDir(projectDir, root string) string {
	info := projectversion.Current(projectDir)
	if err := migrateLegacyStore(root, info.Default); err != nil && hasLegacyStore(root) {
		// Another process holds the old store; keep answering from it until it lets go.
		return root
	}
	return VersionStoreDir(root, info.Version)
}

// ensureProjectVersionDir is projectVersionDir for a writer: a version that has no
// store yet starts as a copy of the default version's, so indexing it only has to
// reconcile what differs between the two checkouts.
func ensureProjectVersionDir(projectDir, root string) (string, error) {
	info := projectversion.Current(projectDir)
	if err := migrateLegacyStore(root, info.Default); err != nil {
		return "", err
	}
	dir := VersionStoreDir(root, info.Version)
	if info.IsDefault() || exists(dir) {
		return dir, nil
	}
	source := VersionStoreDir(root, info.Default)
	if !exists(source) {
		return dir, nil
	}
	if err := forkVersionStore(source, dir); err != nil {
		return "", fmt.Errorf("creating the %q store from %q: %w", info.Version, info.Default, err)
	}
	return dir, nil
}

// hasLegacyStore reports a store root that still holds a store written before versions,
// i.e. anything beside the versions directory and lifecycle lock files.
func hasLegacyStore(root string) bool {
	return len(legacyEntries(root)) > 0
}

func legacyEntries(root string) []os.DirEntry {
	entries, err := os.ReadDir(root)
	if err != nil {
		return nil
	}
	out := entries[:0]
	for _, e := range entries {
		if e.Name() == versionsNamespace || strings.HasSuffix(e.Name(), ".lock") {
			continue
		}
		out = append(out, e)
	}
	return out
}

// migrateLegacyStore moves a pre-version store into the default version. Stores that
// record their own absolute location — the graph's mount DDL and manifests — are
// rewritten to the new one.
func migrateLegacyStore(root, defaultVersion string) error {
	if !hasLegacyStore(root) {
		return nil
	}
	_, guard, err := storelifecycle.TryAcquire(context.Background(), root)
	if err != nil {
		return fmt.Errorf("moving the unversioned store at %s into version %q: %w", root, defaultVersion, err)
	}
	defer guard.Release()
	entries := legacyEntries(root)
	if len(entries) == 0 {
		return nil
	}
	target := VersionStoreDir(root, defaultVersion)
	if exists(target) {
		// A default version already exists, so the leftovers are an older copy of it.
		// Keep them out of the way instead of merging two stores.
		target = filepath.Join(root, versionsNamespace, ".legacy-"+randomSuffix())
	}
	if err := os.MkdirAll(target, 0o755); err != nil {
		return err
	}
	for _, e := range entries {
		if err := os.Rename(filepath.Join(root, e.Name()), filepath.Join(target, e.Name())); err != nil {
			return fmt.Errorf("moving %s: %w", e.Name(), err)
		}
	}
	return rewriteStoreLocation(target, root, target)
}

// forkVersionStore creates target as a copy of source. Data files are hard-linked:
// every writer of these stores replaces a file by rename and never edits one in place
// (Lance fragments, manifests and indexes, Icebug Parquet, parse shards), so a link is
// a copy that costs nothing. The small metadata files that name their own directory
// are copied and pointed at target instead.
func forkVersionStore(source, target string) error {
	waitCtx, cancel := context.WithTimeout(context.Background(), forkLockWait)
	defer cancel()
	_, guard, err := storelifecycle.Acquire(waitCtx, source)
	if err != nil {
		// The default version is busy for longer than a fork is worth waiting; the new
		// version simply indexes from nothing.
		return nil
	}
	defer guard.Release()
	if exists(target) {
		return nil
	}
	staging := target + ".fork." + randomSuffix()
	if err := copyTree(source, staging); err != nil {
		_ = os.RemoveAll(staging)
		return err
	}
	if err := rewriteStoreLocation(staging, source, target); err != nil {
		_ = os.RemoveAll(staging)
		return err
	}
	if err := os.Rename(staging, target); err != nil {
		_ = os.RemoveAll(staging)
		if exists(target) {
			return nil // another process forked the same version first
		}
		return err
	}
	return nil
}

// isLocationMetadata names the files that record their store's absolute location.
func isLocationMetadata(name string) bool {
	ext := strings.ToLower(filepath.Ext(name))
	return ext == ".json" || ext == ".cypher"
}

// isTransient names in-flight publication directories a fork must not inherit.
func isTransient(name string) bool {
	return strings.Contains(name, ".tmp.") || strings.Contains(name, ".backup.") ||
		strings.Contains(name, ".fork.") || strings.Contains(name, ".discard.")
}

func copyTree(source, target string) error {
	return filepath.WalkDir(source, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		if rel != "." && isTransient(d.Name()) {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		dst := filepath.Join(target, rel)
		if d.IsDir() {
			return os.MkdirAll(dst, 0o755)
		}
		if !d.Type().IsRegular() {
			return nil
		}
		if !isLocationMetadata(d.Name()) {
			if err := os.Link(path, dst); err == nil {
				return nil
			}
		}
		return copyFile(path, dst)
	})
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		_ = out.Close()
		return err
	}
	return out.Close()
}

// rewriteStoreLocation repoints the location metadata under dir from oldDir to newDir.
// A graph's mount DDL names its Parquet directory as a Cypher string literal and its
// manifests as JSON strings, so all three spellings of the path are replaced.
func rewriteStoreLocation(dir, oldDir, newDir string) error {
	oldAbs, err := filepath.Abs(oldDir)
	if err != nil {
		return err
	}
	newAbs, err := filepath.Abs(newDir)
	if err != nil {
		return err
	}
	if oldAbs == newAbs {
		return nil
	}
	replacer := strings.NewReplacer(
		jsonEscaped(oldAbs), jsonEscaped(newAbs),
		cypherEscaped(oldAbs), cypherEscaped(newAbs),
		oldAbs, newAbs,
	)
	return filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || !isLocationMetadata(d.Name()) {
			return err
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		updated := replacer.Replace(string(data))
		if updated == string(data) {
			return nil
		}
		tmp := path + ".tmp." + randomSuffix()
		if err := os.WriteFile(tmp, []byte(updated), 0o644); err != nil {
			return err
		}
		return os.Rename(tmp, path)
	})
}

func jsonEscaped(s string) string {
	b, _ := json.Marshal(s)
	return string(b[1 : len(b)-1])
}

// cypherEscaped matches ladybugstore.EscapeLiteral, which writes the mount DDL.
func cypherEscaped(s string) string {
	return strings.ReplaceAll(strings.ReplaceAll(s, `\`, `\\`), `'`, `\'`)
}

func randomSuffix() string {
	var b [6]byte
	_, _ = rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

func exists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
