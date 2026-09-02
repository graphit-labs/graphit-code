package memory

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// The memory store's backend, after git.
//
// THE CHAIN IS UNCHANGED EXCEPT AT THE REMOTE END:
//
//	before: branch memory/<scope>/<id>                  --fetch/rebase--> raw dir (TRUTH) --compile--> wiki
//	after:  s3://<bucket>/<prefix>/memory/<scope>/<id>/ --sync-->         raw dir (TRUTH) --compile--> wiki
//
// A "scope path" is what a branch name used to be — `memory/<scope>/<id>`. It survives as the
// addressing scheme because it was never really a git concept: it is a two-level namespace, and it
// is already a valid key path, so the translation to a prefix is the identity.
//
// WHAT CALLERS CAN FEEL, and it is three things:
//
//   - There is no commit, so there is no commit message. CommitAndPush became Publish, whose
//     argument is only logged — the audit trail moved INTO the memory instead: see the `revision`,
//     `previous` and `updated_by` frontmatter fields, and HistoryPath.
//   - Pull MERGES rather than mirrors. A memory written locally and not yet uploaded must survive a
//     pull, so nothing local is deleted to match the remote; removal is driven only by RemoveFile.
//     This is the opposite of the Hub registry, which mirrors on purpose.
//   - Conflict is per object, last writer wins. Each memory is a file named by a ULID, so two units
//     ADDING memories touch different objects and cannot conflict at all. Only editing or deleting
//     the same memory races, and there last-writer-wins is what `rebase -X ours` approximated.
const memoryPrefix = "memory"

var (
	memRevisionCacheMu  sync.Mutex
	memRevisionCacheMap = map[string]memRevisionEntry{}
	memRevisionCacheTTL = 30 * time.Second
)

type memRevisionEntry struct {
	revision string
	at       time.Time
}

func cachedRemoteRevision(key string) (string, bool) {
	memRevisionCacheMu.Lock()
	defer memRevisionCacheMu.Unlock()
	e, ok := memRevisionCacheMap[key]
	if !ok || time.Since(e.at) > memRevisionCacheTTL {
		return "", false
	}
	return e.revision, true
}

func setCachedRemoteRevision(key, revision string) {
	memRevisionCacheMu.Lock()
	memRevisionCacheMap[key] = memRevisionEntry{revision: revision, at: time.Now()}
	memRevisionCacheMu.Unlock()
}

// MemoryStore is the memory scopes' persistence backend.
//
// A missing bucket is local-only mode rather than an error, which is exactly what an unset
// memory.repo did: every scope still works against its raw directory, and nothing is uploaded.
type MemoryStore struct {
	Logger *slog.Logger

	objects *s3store.Store
	rawBase string
}

func (m *MemoryStore) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

// NewMemoryStore resolves the Hub bucket and the local raw directory root.
//
// Memory shares the Hub's bucket under the `memory/` prefix — it was one of the five things the
// Hub's git repository carried, and all five moved together. So there is no memory-specific bucket
// key, and memory.repo is gone.
//
// It takes no context because none of its callers has one: building the AWS client only loads
// configuration and touches no network.
func NewMemoryStore() (*MemoryStore, error) {
	base := store.MemoryRawRoot()
	if base == "" {
		return nil, fmt.Errorf("resolving memory directory root")
	}
	m := &MemoryStore{rawBase: base}

	cfg := config.HubS3Config()
	if !cfg.Configured() {
		return m, nil
	}
	objects, err := s3store.New(context.Background(), cfg)
	if err != nil {
		if errors.Is(err, s3store.ErrNotConfigured) {
			return m, nil
		}
		return nil, err
	}
	m.objects = objects
	return m, nil
}

// Configured reports whether there is a remote at all.
func (m *MemoryStore) Configured() bool { return m.objects != nil }

// Dir is the root holding every scope's raw directory.
func (m *MemoryStore) Dir() string { return m.rawBase }

// EnsureInitialised has nothing left to initialise.
//
// It ran eight git invocations once — `git init`, a bootstrap commit, a remote, a prune — before the
// first memory could be read. Those went when memory left git. Then it created the raw directory
// root, and that went too: a scope's store is a Lance table, and opening one CREATES it, so there is
// no directory to prepare in advance.
//
// Creating it anyway was not harmless. With the raw store retired the directory came back empty on
// every run, which reads as "the raw store is still a thing" to anyone looking at the global
// directory — and it is the kind of residue that makes a later reader restore a mechanism instead of
// deleting its last thread.
func (m *MemoryStore) EnsureInitialised() error { return nil }

// EnsureInitialisedFast is EnsureInitialised. It survives as a separate name because callers choose
// between them to mean "skip the network", and initialisation no longer touches it at all.
func (m *MemoryStore) EnsureInitialisedFast() error { return m.EnsureInitialised() }

// remotePrefix is the key prefix holding one scope's memories.
//
// A scope path is already `memory/<scope>/<id>`, so the translation is the identity and the layout
// the git branches described is preserved exactly. A leading `memory/` is stripped first so it
// cannot be doubled by a caller that passes the path either way.
func remotePrefix(scopePath string) string {
	trimmed := strings.Trim(scopePath, "/")
	trimmed = strings.TrimPrefix(trimmed, memoryPrefix+"/")
	return s3store.JoinKey(memoryPrefix, trimmed)
}

// OpenScope opens a scope's raw directory and syncs it from the remote.
func (m *MemoryStore) OpenScope(scopePath string) (*ScopeStore, error) {
	return m.openScope(scopePath, false)
}

// OpenScopeLocal opens a scope's raw directory without touching the network.
func (m *MemoryStore) OpenScopeLocal(scopePath string) (*ScopeStore, error) {
	return m.openScope(scopePath, true)
}

// HasLocalScope reports whether a scope's raw directory already exists.
//
// It checks the DIRECTORY, not a `.git` inside it — which is what it used to check, and what would
// now report false for every scope.
func (m *MemoryStore) HasLocalScope(scopePath string) bool {
	info, err := os.Stat(m.scopeDir(scopePath))
	return err == nil && info.IsDir()
}

func (m *MemoryStore) openScope(scopePath string, skipNetwork bool) (*ScopeStore, error) {
	if err := m.EnsureInitialised(); err != nil {
		return nil, err
	}
	dir := m.scopeDir(scopePath)
	created := false
	if _, err := os.Stat(dir); err != nil {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, fmt.Errorf("creating memory directory for %q: %w", scopePath, err)
		}
		created = true
	}

	s := &ScopeStore{store: m, scopePath: scopePath, dir: dir, logger: m.Logger}

	if created {
		if err := m.RegisterScope(scopePath, dir); err != nil {
			m.log().Warn("register scope failed", "scope", scopePath, "error", err)
		}
	}
	if !skipNetwork {
		if err := s.Pull(); err != nil {
			s.log().Warn("pull failed", "scope", scopePath, "error", err)
		}
	}
	return s, nil
}

func (m *MemoryStore) scopeDir(scopePath string) string {
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(scopePath)
	return filepath.Join(m.rawBase, safe)
}

// ScopeDir returns the local directory holding a scope's raw memories.
func (m *MemoryStore) ScopeDir(scopePath string) string { return m.scopeDir(scopePath) }

// remoteRevision identifies the remote state of one scope, so an unchanged prefix is not downloaded
// again.
//
// It replaces `git ls-remote` and the commit SHA it returned. One listing of small markdown objects
// is cheap, and hashing key+size detects an added, removed or resized memory. A same-size rewrite of
// one memory is NOT detected — accepted, and bounded: a memory carries a revision counter and an
// updated_at, so an edit that leaves the byte count identical is not a case that occurs.
func (m *MemoryStore) remoteRevision(ctx context.Context, scopePath string) string {
	if !m.Configured() {
		return ""
	}
	key := remotePrefix(scopePath)
	if rev, ok := cachedRemoteRevision(key); ok {
		return rev
	}
	objs, err := m.objects.List(ctx, key)
	if err != nil {
		return ""
	}
	lines := make([]string, 0, len(objs))
	for _, o := range objs {
		lines = append(lines, fmt.Sprintf("%s:%d", o.Key, o.Size))
	}
	sort.Strings(lines)
	sum := sha256.Sum256([]byte(strings.Join(lines, "\n")))
	rev := hex.EncodeToString(sum[:])
	setCachedRemoteRevision(key, rev)
	return rev
}

// ExtractScopeDir copies a subdirectory of a scope's raw memories elsewhere.
func (m *MemoryStore) ExtractScopeDir(scopePath, relDir, destDir string) error {
	s, err := m.OpenScope(scopePath)
	if err != nil {
		return err
	}
	src := filepath.Join(s.dir, relDir)
	if _, err := os.Stat(src); err != nil {
		return os.MkdirAll(destDir, 0o755)
	}
	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return err
	}
	return copyDirRecursive(src, destDir)
}

// ScopeStore is one scope's raw memory directory, backed by a key prefix.
type ScopeStore struct {
	store     *MemoryStore
	scopePath string
	dir       string
	logger    *slog.Logger

	// removed collects paths deleted locally, so Publish can delete their objects too. A deletion
	// cannot be inferred from the directory afterwards — the file is already gone.
	mu      sync.Mutex
	removed []string
}

func (s *ScopeStore) log() *slog.Logger { return slogutil.Resolve(s.logger) }

func (s *ScopeStore) Dir() string { return s.dir }

// Pull brings the remote's memories into the raw directory, MERGING.
//
// Nothing local is deleted to match the remote. A memory just written and not yet uploaded would be
// the thing deleted, and losing it is worse than keeping a memory somebody removed elsewhere —
// which the next Publish from that unit removes anyway.
func (s *ScopeStore) Pull() error {
	if !s.store.Configured() {
		return nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()

	if s.store.remoteRevision(ctx, s.scopePath) == "" {
		// Nothing published for this scope yet, or the listing failed. Either way there is nothing
		// to merge, and an empty scope is a normal state.
		return nil
	}
	if err := s.store.objects.DownloadPrefix(ctx, remotePrefix(s.scopePath), s.dir); err != nil {
		return fmt.Errorf("syncing memory scope %q: %w", s.scopePath, err)
	}
	return nil
}

func (s *ScopeStore) WriteFile(relPath string, data []byte) error {
	full := filepath.Join(s.dir, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		return err
	}
	return os.WriteFile(full, data, 0o644)
}

func (s *ScopeStore) ReadFile(relPath string) ([]byte, error) {
	return os.ReadFile(filepath.Join(s.dir, relPath))
}

// RemoveFile deletes a memory locally and records it for Publish to delete remotely.
func (s *ScopeStore) RemoveFile(relPath string) error {
	if err := os.Remove(filepath.Join(s.dir, relPath)); err != nil {
		return err
	}
	s.mu.Lock()
	s.removed = append(s.removed, filepath.ToSlash(relPath))
	s.mu.Unlock()
	return nil
}

func (s *ScopeStore) ListDir(relDir string) ([]os.DirEntry, error) {
	return os.ReadDir(filepath.Join(s.dir, relDir))
}

// Publish uploads the scope's memories and deletes the ones removed since it was opened.
//
// `reason` replaces the commit message and is LOGGED, NOT STORED: object storage has no commit to
// carry it. The audit trail it used to imply lives in the memory itself now — `revision`,
// `previous` and `updated_by` in the frontmatter, with each superseded version archived under
// HistoryPath.
//
// The upload runs in the background, as the push it replaces did, so writing a memory never waits
// on the network. The raw directory is the truth and it is already written when this returns.
func (s *ScopeStore) Publish(reason string) error {
	if !s.store.Configured() {
		return nil
	}

	s.mu.Lock()
	removed := append([]string(nil), s.removed...)
	s.removed = nil
	s.mu.Unlock()

	pendingPushes.Add(1)
	go func() {
		defer pendingPushes.Done()

		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
		defer cancel()

		key := remotePrefix(s.scopePath)
		for _, rel := range removed {
			if err := s.store.objects.Delete(ctx, s3store.JoinKey(key, rel)); err != nil {
				s.log().Warn("deleting memory object failed", "scope", s.scopePath, "path", rel, "error", err)
			}
		}
		if err := s.uploadDir(ctx, key); err != nil {
			s.log().Warn("publishing memories failed", "scope", s.scopePath, "reason", reason, "error", err)
			return
		}
		// The listing this scope cached is stale the moment anything was written.
		setCachedRemoteRevision(key, "")
	}()
	return nil
}

// uploadDir uploads every file in the raw directory under the scope's key.
func (s *ScopeStore) uploadDir(ctx context.Context, key string) error {
	return filepath.WalkDir(s.dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		rel, relErr := filepath.Rel(s.dir, path)
		if relErr != nil {
			return relErr
		}
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		return s.store.objects.Put(ctx, s3store.JoinKey(key, filepath.ToSlash(rel)), data)
	})
}

var pendingPushes sync.WaitGroup

// WaitForPendingPushes blocks until every background upload has finished. The CLI and the daemon
// call it on shutdown so a memory written by the last command is not lost with the process.
func WaitForPendingPushes() { pendingPushes.Wait() }

// Prune removes a scope's raw directory and deregisters it.
//
// IT DOES NOT DELETE THE REMOTE PREFIX, and that is deliberate: `git branch -D` was local too.
// Pruning reclaims local disk for a scope this unit no longer tracks, and a scope another unit
// still uses must survive it.
func (s *ScopeStore) Prune() error {
	if _, err := s.store.DeregisterScope(s.scopePath, s.dir); err != nil {
		s.log().Warn("prune: deregister scope failed", "scope", s.scopePath, "error", err)
	}
	if err := os.RemoveAll(s.dir); err != nil {
		s.log().Warn("prune: remove directory failed", "dir", s.dir, "error", err)
	}
	return nil
}

// copyDirRecursive copies a tree, preserving file modes.
func copyDirRecursive(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, relErr := filepath.Rel(src, path)
		if relErr != nil {
			return relErr
		}
		dest := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(dest, 0o755)
		}
		info, infoErr := d.Info()
		if infoErr != nil {
			return infoErr
		}
		return copyFileData(path, dest, info.Mode())
	})
}

// copyFileData copies one file, preserving its mode.
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
