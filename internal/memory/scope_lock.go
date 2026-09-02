package memory

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type scopeMeta struct {
	Refs     []string `json:"refs"`
	LastUsed string   `json:"lastUsed"`
}

// scopeLockFile records which projects reference which memory scopes, so a scope's local
// directory can be reclaimed once nothing points at it.
type scopeLockFile struct {
	Version int                   `json:"version"`
	Scopes  map[string]*scopeMeta `json:"scopes"`
}

func scopeLockPath() string {
	return filepath.Join(brand.GlobalDir(), "memory.lock.json")
}

func loadMemLock() (*scopeLockFile, error) {
	data, err := os.ReadFile(scopeLockPath())
	if os.IsNotExist(err) {
		return &scopeLockFile{Version: 1, Scopes: make(map[string]*scopeMeta)}, nil
	}
	if err != nil {
		return nil, err
	}
	var lf scopeLockFile
	if err := json.Unmarshal(data, &lf); err != nil {
		return &scopeLockFile{Version: 1, Scopes: make(map[string]*scopeMeta)}, nil
	}
	if lf.Scopes == nil {
		lf.Scopes = make(map[string]*scopeMeta)
	}
	return &lf, nil
}

func saveMemLock(lf *scopeLockFile) error {
	path := scopeLockPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(lf, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}

func (m *MemoryStore) activeScopes() ([]string, error) {
	lf, err := loadMemLock()
	if err != nil {
		return nil, err
	}
	var active []string
	for scopePath, meta := range lf.Scopes {
		if len(meta.Refs) > 0 {
			active = append(active, scopePath)
		}
	}
	return active, nil
}

func (m *MemoryStore) RegisterScope(scopePath, ref string) error {
	lf, err := loadMemLock()
	if err != nil {
		return err
	}
	meta := lf.Scopes[scopePath]
	if meta == nil {
		meta = &scopeMeta{}
		lf.Scopes[scopePath] = meta
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

func (m *MemoryStore) DeregisterScope(scopePath, ref string) (bool, error) {
	lf, err := loadMemLock()
	if err != nil {
		return false, err
	}
	meta := lf.Scopes[scopePath]
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
		delete(lf.Scopes, scopePath)

		m.pruneLocalScope(scopePath)
	}
	return unused, saveMemLock(lf)
}

func (m *MemoryStore) ValidateScopeRefs() (int, error) {
	lf, err := loadMemLock()
	if err != nil {
		return 0, err
	}
	cleaned := 0
	for scopePath, meta := range lf.Scopes {
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
			delete(lf.Scopes, scopePath)
			m.pruneLocalScope(scopePath)
		}
	}
	if cleaned > 0 {
		return cleaned, saveMemLock(lf)
	}
	return 0, nil
}

// PruneScope deletes a scope's local directory and its entry in the scope lock, regardless of
// what is registered against it.
//
// DESTRUCTIVE, and deliberately unconditional: DeregisterScope only prunes when the last
// reference goes away, which is right for a scope somebody might still be using and wrong for
// one that should never have been created. This is for the second case — reclaiming the project
// scope an ephemeral live search session used to acquire — where leaving the scope behind
// because its bookkeeping looks alive is the bug, not the safeguard.
//
// The caller is responsible for knowing the scope is disposable. Nothing here checks, because
// nothing here can: a scope holding a session's memories and one holding a team's are the same
// shape. And it is LOCAL ONLY — the remote prefix survives, because another unit may still be
// reading the table it addresses.
func (m *MemoryStore) PruneScope(scope, scopeID string) error {
	if scope == "" || scopeID == "" {
		return fmt.Errorf("memory scope and id are required")
	}
	scopePath := fmt.Sprintf("memory/%s/%s", scope, scopeID)

	lf, err := loadMemLock()
	if err == nil {
		if _, ok := lf.Scopes[scopePath]; ok {
			delete(lf.Scopes, scopePath)
			_ = saveMemLock(lf)
		}
	}
	m.pruneLocalScope(scopePath)
	return nil
}

// pruneLocalScope removes a scope's local table directory.
//
// It used to also run `git branch -D`. There is no branch now, and no ref to delete: the
// directory and the lock entry are the whole of this unit's record of the scope.
//
// 🔒 IT REMOVES THE TABLE, and it used to remove the raw markdown directory — which stopped
// existing without this noticing, so reclaiming a scope left the only copy of its data behind while
// deleting the bookkeeping that said the scope was there.
func (m *MemoryStore) pruneLocalScope(scopePath string) {
	dir := m.scopeDir(scopePath)
	if err := os.RemoveAll(dir); err != nil {
		m.log().Warn("prune: remove table directory failed", "dir", dir, "error", err)
	}
}

func (m *MemoryStore) ActiveScopes() ([]string, error) {
	return m.activeScopes()
}

func (m *MemoryStore) ScopeSummary() (map[string][]string, error) {
	lf, err := loadMemLock()
	if err != nil {
		return nil, err
	}
	out := make(map[string][]string, len(lf.Scopes))
	for scopePath, meta := range lf.Scopes {
		out[scopePath] = meta.Refs
	}
	return out, nil
}
