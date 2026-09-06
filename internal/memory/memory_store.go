package memory

import (
	"fmt"
	"log/slog"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// MemoryStore is the memory scopes' persistence backend.
//
// A missing bucket is local-only mode rather than an error, which is exactly what an unset
// memory.repo did: every scope still works, against a table in the global directory instead of one
// in object storage, and nothing is uploaded.
type MemoryStore struct {
	Logger *slog.Logger

	tableBase string
}

func (m *MemoryStore) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

// NewMemoryStore resolves the local root every scope's table lives under.
//
// IT NO LONGER BUILDS AN S3 CLIENT. It did while memory was markdown objects this code uploaded one
// by one; a scope is a Lance table now, and the table reaches S3 through the URI it was opened with.
// A second client here would be a second set of credentials resolving to the same bucket, and the
// only thing it could still answer — "is a bucket configured" — is config.HubS3Config().Configured().
//
// Memory shares the Hub's bucket under the `memory/` prefix — it was one of the five things the
// Hub's git repository carried, and all five moved together. So there is no memory-specific bucket
// key, and memory.repo is gone.
func NewMemoryStore() (*MemoryStore, error) {
	base := store.MemoryTableRoot()
	if base == "" {
		return nil, fmt.Errorf("resolving memory directory root")
	}
	return &MemoryStore{tableBase: base}, nil
}

// Dir is the root holding every scope's local table.
func (m *MemoryStore) Dir() string { return m.tableBase }

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

func (m *MemoryStore) scopeDir(scopePath string) string {
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(scopePath)
	return filepath.Join(m.tableBase, safe)
}
