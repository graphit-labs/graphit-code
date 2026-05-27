package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

func (k *LadybugBackend) DropAndRecreate(ctx interface{ Value(any) any }) error {
	k.mu.Lock()

	if k.conn != nil {
		k.conn.Close()
		k.conn = nil
	}
	if k.db != nil {
		k.db.Close()
		k.db = nil
	}

	k.once = sync.Once{}

	k.mu.Unlock()

	dbDir := k.cfg.DBPath
	if err := os.RemoveAll(dbDir); err != nil {
		return fmt.Errorf("remove db: %w", err)
	}

	k.mu.Lock()
	defer k.mu.Unlock()
	return k.ensureConnected()
}

func (k *LadybugBackend) AtomicSwapDB(newDBPath string) error {
	currentPath := k.cfg.DBPath
	oldPath := currentPath + ".old"

	_ = os.RemoveAll(oldPath)

	if err := os.Rename(currentPath, oldPath); err != nil && !os.IsNotExist(err) {

		return fmt.Errorf("atomic swap: rename current→old: %w", err)
	}

	if err := os.Rename(newDBPath, currentPath); err != nil {

		if restoreErr := os.Rename(oldPath, currentPath); restoreErr != nil {
			return fmt.Errorf("atomic swap CRITICAL: new→current failed (%v) AND restore failed (%v)", err, restoreErr)
		}
		return fmt.Errorf("atomic swap: rename new→current: %w", err)
	}

	_ = os.RemoveAll(oldPath)
	_ = os.Remove(currentPath + ".wal")

	return nil
}

func CleanupInterruptedSwap(dbPath string) {
	_ = os.RemoveAll(dbPath + ".old")

	matches, _ := filepath.Glob(dbPath + ".*")
	for _, m := range matches {
		if m == dbPath+".wal" || strings.HasPrefix(m, dbPath+".search.sqlite") {
			continue
		}
		_ = os.RemoveAll(m)
	}

	_ = os.RemoveAll(dbPath + ".staging")
}
