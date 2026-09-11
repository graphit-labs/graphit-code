//go:build lancedb

package ast

import (
	"context"
	"fmt"
	"io/fs"
	"path/filepath"
	"testing"
	"time"
)

// A sequence of incrementals leaves fragments and superseded versions behind, and nothing else in
// the pipeline reclaims them. This pins that Maintain does.
//
// The assertion is on what the ENGINE reports, not on a count of files under the table's data
// directory. That count is not a fragment count: compaction writes the merged fragment and leaves
// the ones it replaced in place until their versions are pruned, so it goes UP before it goes
// down. An earlier version of Maintain used it as a threshold and would have compacted on every
// write forever.
func TestMaintainCompactsFragmentsAndPrunesVersions(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)

	entries := make([]*parseCacheEntry, 0, 40)
	for i := 0; i < 40; i++ {
		entries = append(entries, entryWith(fmt.Sprintf("pkg/f%d.go", i), "package pkg",
			cachedEntity{Name: fmt.Sprintf("Fn%d", i)}))
	}
	cache := newShardCacheForTest(t, entries...)
	if err := idx.RebuildFromCache(ctx, cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	for i := 0; i < 40; i++ {
		rel := fmt.Sprintf("pkg/f%d.go", i)
		if err := idx.UpdateIncremental(ctx, cache, []string{rel}, nil, nil); err != nil {
			t.Fatalf("incremental %s: %v", rel, err)
		}
	}

	sizeBefore := tableBytes(t, idx, lanceEntitiesTable)

	comp, err := idx.entities.Compact(ctx)
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if comp.FragmentsRemoved == 0 {
		t.Errorf("40 incrementals left nothing to merge: %+v", comp)
	}
	if comp.FragmentsAdded >= comp.FragmentsRemoved {
		t.Errorf("compaction did not reduce the fragment count: %+v", comp)
	}

	time.Sleep(1100 * time.Millisecond)
	pruned, err := idx.entities.PruneVersions(ctx, time.Second)
	if err != nil {
		t.Fatalf("prune: %v", err)
	}
	t.Logf("compaction=%+v prune=%+v bytes %d -> %d",
		comp, pruned, sizeBefore, tableBytes(t, idx, lanceEntitiesTable))

	if after := tableBytes(t, idx, lanceEntitiesTable); after >= sizeBefore {
		t.Errorf("compaction and pruning reclaimed nothing: %d bytes before, %d after",
			sizeBefore, after)
	}

	entCount, _, err := idx.Counts(ctx)
	if err != nil {
		t.Fatalf("counts after compaction: %v", err)
	}
	if entCount != 40 {
		t.Errorf("compaction changed the row count: got %d, want 40", entCount)
	}
	if got := searchNames(t, idx, "Fn7", 5); !hasName(got, "Fn7") {
		t.Errorf("search broke after compaction: %v", got)
	}
}

// Maintain has to be cheap when there is nothing to do, because it runs after every index write.
// With no fragments to merge the engine reports zero and has only read manifest metadata.
func TestMaintainOnAnAlreadyCompactStoreDoesNothing(t *testing.T) {
	ctx := context.Background()
	idx := newLanceIndexForTest(t)
	cache := newShardCacheForTest(t,
		entryWith("a.go", "package a", cachedEntity{Name: "onlyOne"}))
	if err := idx.RebuildFromCache(ctx, cache, nil); err != nil {
		t.Fatalf("rebuild: %v", err)
	}

	idx.Maintain(ctx)

	comp, err := idx.entities.Compact(ctx)
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if comp.FragmentsRemoved != 0 {
		t.Errorf("a store Maintain just compacted still had %d fragments to merge",
			comp.FragmentsRemoved)
	}
	if got := searchNames(t, idx, "onlyOne", 5); !hasName(got, "onlyOne") {
		t.Errorf("search broke: %v", got)
	}
}

func tableBytes(t *testing.T, idx *SearchIndex, table string) int64 {
	t.Helper()
	root := filepath.Join(idx.store.URI(), table+".lance")
	var total int64
	err := filepath.WalkDir(root, func(_ string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return nil
		}
		total += info.Size()
		return nil
	})
	if err != nil {
		t.Fatalf("measuring %s: %v", table, err)
	}
	return total
}
