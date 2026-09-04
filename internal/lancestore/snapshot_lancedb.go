//go:build lancedb

package lancestore

import (
	"context"
	"fmt"
	"sort"
	"time"
)

// CompactLatestSnapshot rewrites a local store for publication and removes every superseded
// table version. It must run on a private staging copy because pruning invalidates old readers.
func CompactLatestSnapshot(ctx context.Context, uri string) (SnapshotResult, error) {
	cfg := Config{URI: uri}
	if cfg.IsRemote() {
		return SnapshotResult{}, fmt.Errorf("lancestore: latest snapshot requires a local staging directory")
	}
	st, err := Open(ctx, cfg)
	if err != nil {
		return SnapshotResult{}, err
	}
	defer st.Close()

	names, err := st.TableNames(ctx)
	if err != nil {
		return SnapshotResult{}, err
	}
	sort.Strings(names)
	out := SnapshotResult{Tables: len(names)}
	tables := make([]*Table, 0, len(names))
	for _, name := range names {
		table, err := st.OpenTable(ctx, name)
		if err != nil {
			return SnapshotResult{}, err
		}
		if _, err := table.Compact(ctx); err != nil {
			return SnapshotResult{}, fmt.Errorf("lancestore: snapshot compacting %s: %w", name, err)
		}
		tables = append(tables, table)
	}

	// The Go binding floors every positive prune window to one second. Wait once after all
	// compactions so their superseded manifests are eligible without delaying once per table.
	timer := time.NewTimer(1100 * time.Millisecond)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return SnapshotResult{}, ctx.Err()
	case <-timer.C:
	}

	for _, table := range tables {
		pruned, err := table.PruneVersions(ctx, time.Second)
		if err != nil {
			return SnapshotResult{}, fmt.Errorf("lancestore: snapshot pruning %s: %w", table.Name(), err)
		}
		out.BytesRemoved += pruned.BytesRemoved
		out.OldVersions += pruned.OldVersions
		versions, err := table.Versions(ctx)
		if err != nil {
			return SnapshotResult{}, fmt.Errorf("lancestore: snapshot verifying %s: %w", table.Name(), err)
		}
		if len(versions) != 1 {
			return SnapshotResult{}, fmt.Errorf("lancestore: snapshot of %s retained %d versions, want 1", table.Name(), len(versions))
		}
	}
	return out, nil
}
