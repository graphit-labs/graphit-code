package memory

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type scopeMeta struct {
	Refs     []string `json:"refs"`
	LastUsed string   `json:"lastUsed"`
}

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

// PruneScope deletes a memory scope's local tables on an explicit request, and its entry in
// the legacy scope lock file when one is still recorded there.
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

func (m *MemoryStore) pruneLocalScope(scopePath string) {
	dir := m.scopeDir(scopePath)
	if err := os.RemoveAll(dir); err != nil {
		m.log().Warn("prune: remove table directory failed", "dir", dir, "error", err)
	}
}
