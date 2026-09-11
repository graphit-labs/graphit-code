//go:build !lancedb

package lancestore

import (
	"context"
	"time"
)

// The build WITHOUT the native library.
//
// Every constructor fails with ErrNotBuilt and every method is a no-op returning the same, so
// the package type-checks and callers compile — which is the point. The native is ~230 MiB and
// is built from source by `make fetch-lancedb`; requiring it for `go build ./...` would break
// the tree for anyone who has not run that target.
//
// This file and store_lancedb.go MUST expose the same surface. A method added to one and not
// the other breaks the build of whichever configuration is not being tested at that moment,
// which is exactly the kind of breakage nobody notices until CI runs the other one.

// Store is a LanceDB database. In this build it cannot be opened.
type Store struct{}

// Table is a table in a Store. In this build it cannot be reached.
type Table struct{}

// Open reports that the binary carries no LanceDB.
func Open(_ context.Context, _ Config) (*Store, error) { return nil, ErrNotBuilt }

// Available reports whether this build can open a store at all. It is the one call that does
// NOT error, so a caller can degrade deliberately instead of discovering it from an error.
func Available() bool { return false }

func (s *Store) Close() error   { return nil }
func (s *Store) Remote() bool   { return false }
func (s *Store) ReadOnly() bool { return false }
func (s *Store) URI() string    { return "" }

func (s *Store) TableNames(_ context.Context) ([]string, error)        { return nil, ErrNotBuilt }
func (s *Store) DropTable(_ context.Context, _ string) error           { return ErrNotBuilt }
func (s *Store) OpenTable(_ context.Context, _ string) (*Table, error) { return nil, ErrNotBuilt }
func (s *Store) CreateTable(_ context.Context, _ string, _ Schema) (*Table, error) {
	return nil, ErrNotBuilt
}
func (s *Store) EnsureTable(_ context.Context, _ string, _ Schema) (*Table, error) {
	return nil, ErrNotBuilt
}
func (s *Store) CloneTable(_ context.Context, _, _ string, _ CloneOptions) (*Table, error) {
	return nil, ErrNotBuilt
}

func (t *Table) Close() error                                  { return nil }
func (t *Table) Name() string                                  { return "" }
func (t *Table) Schema() Schema                                { return Schema{} }
func (t *Table) Count(_ context.Context) (int64, error)        { return 0, ErrNotBuilt }
func (t *Table) Append(_ context.Context, _ []Row) error       { return ErrNotBuilt }
func (t *Table) DeleteWhere(_ context.Context, _ string) error { return ErrNotBuilt }
func (t *Table) DeleteByKey(_ context.Context, _ string, _ []string) error {
	return ErrNotBuilt
}
func (t *Table) Upsert(_ context.Context, _ string, _ []Row) error { return ErrNotBuilt }
func (t *Table) ReplaceSnapshot(_ context.Context, _ []string, _ []Row) (uint64, error) {
	return 0, ErrNotBuilt
}
func (t *Table) Rows(_ context.Context) ([]Row, error) { return nil, ErrNotBuilt }
func (t *Table) Refresh(_ context.Context) error       { return ErrNotBuilt }
func (t *Table) Merge(_ context.Context, _ MergeOptions, _ []Row) (MergeResult, error) {
	return MergeResult{}, ErrNotBuilt
}
func (t *Table) EnsureIndexes(_ context.Context, _ ...Index) error { return ErrNotBuilt }
func (t *Table) DropIndex(_ context.Context, _ Index) error        { return ErrNotBuilt }
func (t *Table) Search(_ context.Context, _ Query) ([]Hit, error)  { return nil, ErrNotBuilt }

// FoldNewRowsIntoIndexes is unavailable without the lancedb tag.
func (t *Table) FoldNewRowsIntoIndexes(context.Context) error { return ErrNotBuilt }

// Compact is unavailable without the lancedb tag.
func (t *Table) Compact(context.Context) (CompactionResult, error) {
	return CompactionResult{}, ErrNotBuilt
}

// PruneVersions is unavailable without the lancedb tag.
func (t *Table) PruneVersions(context.Context, time.Duration) (PruneResult, error) {
	return PruneResult{}, ErrNotBuilt
}

// The time-travel surface is unavailable without the lancedb tag.
func (t *Table) Versions(context.Context) ([]Version, error)    { return nil, ErrNotBuilt }
func (t *Table) CheckoutVersion(context.Context, uint64) error  { return ErrNotBuilt }
func (t *Table) CheckoutLatest(context.Context) error           { return ErrNotBuilt }
func (t *Table) RestoreVersion(context.Context, uint64) error   { return ErrNotBuilt }
func (t *Table) CurrentVersion(context.Context) (uint64, error) { return 0, ErrNotBuilt }
func (t *Table) PutTag(context.Context, string, uint64) error   { return ErrNotBuilt }
