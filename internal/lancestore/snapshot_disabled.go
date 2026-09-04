//go:build !lancedb

package lancestore

import "context"

func CompactLatestSnapshot(context.Context, string) (SnapshotResult, error) {
	return SnapshotResult{}, ErrNotBuilt
}
