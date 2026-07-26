package ast

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

// TestCloseWithoutCheckpointPersists asks whether the explicit CHECKPOINT that
// Shutdown() runs before every atomic swap is actually required for durability,
// or whether Close() alone flushes.
//
// This matters a lot: on a 35k-file repository that CHECKPOINT is the single
// largest and by far the most variable cost of an incremental rebuild
// (measured 239 ms – 4457 ms, while every other phase stays under ~110 ms).
// AtomicSwapDB deletes the ".wal" sidecar, so anything left unflushed would be
// lost — hence the test writes, closes WITHOUT checkpointing, removes the WAL
// exactly as the swap does, and reopens to see whether the data survived.
func TestCloseWithoutCheckpointPersists(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "ladybugdb")
	ctx := context.Background()

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	if _, err := db.Execute(ctx, "CREATE NODE TABLE T(id INT64, v STRING, PRIMARY KEY(id))", nil); err != nil {
		t.Fatalf("create table: %v", err)
	}
	for i := 1; i <= 50; i++ {
		if _, err := db.Execute(ctx, "CREATE (:T {id: $id, v: $v})", map[string]any{"id": i, "v": "row"}); err != nil {
			t.Fatalf("insert %d: %v", i, err)
		}
	}

	// Close WITHOUT the CHECKPOINT that Shutdown() would issue.
	if err := db.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	// AtomicSwapDB drops the WAL sidecar; replicate that here.
	walPath := dbPath + ".wal"
	walExisted := false
	if _, err := os.Stat(walPath); err == nil {
		walExisted = true
	}
	_ = os.Remove(walPath)
	t.Logf("wal present after Close(): %v", walExisted)

	db2 := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := db2.connect(); err != nil {
		t.Fatalf("reopen: %v", err)
	}
	defer func() { _ = db2.Close() }()

	res, err := db2.Query(ctx, "MATCH (n:T) RETURN count(n) AS c", nil)
	if err != nil {
		t.Fatalf("count after reopen: %v", err)
	}
	var got int64
	if len(res.Records) > 0 {
		switch v := res.Records[0]["c"].(type) {
		case int64:
			got = v
		case int:
			got = int64(v)
		}
	}
	t.Logf("rows after Close()+WAL removal+reopen: %d (want 50)", got)
	if got != 50 {
		t.Errorf("data lost without CHECKPOINT: got %d rows, want 50 — the explicit CHECKPOINT is required", got)
	}
}
