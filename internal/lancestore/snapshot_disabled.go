//go:build !lancedb

package lancestore

import "context"

func CompactLatestSnapshot(context.Context, Config) (SnapshotResult, error) {
	return SnapshotResult{}, ErrNotBuilt
}
