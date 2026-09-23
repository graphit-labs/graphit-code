//go:build lancedb

package dream

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestLatestRunTreatsExistingStoreWithoutTableAsEmpty(t *testing.T) {
	projectDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	projectID := writeDreamTestProject(t, projectDir)
	if err := os.MkdirAll(store.DreamTableDir(projectID), 0o755); err != nil {
		t.Fatal(err)
	}
	record, err := LatestRun(context.Background(), projectDir)
	if err != nil || record != nil {
		t.Fatalf("LatestRun = %#v, %v; want empty ledger", record, err)
	}
}

func TestRunLedgerStoresOnlyOperationalMetadata(t *testing.T) {
	projectDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	ledger, err := openRunLedger(context.Background(), projectDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = ledger.Close() }()
	record := newRunRecord(projectDir, "run-1", "codex", "codex", time.Unix(10, 0))
	record.Status = "completed"
	record.FinishedAt = time.Unix(20, 0).UTC().Format(time.RFC3339Nano)
	record.ToolCalls = 3
	record.MemoryMutationAttempts = 1
	record.TargetIDs = []string{"memory-1"}
	if err := ledger.Put(context.Background(), record); err != nil {
		t.Fatal(err)
	}
	rows, err := ledger.table.Rows(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(rows) != 1 || rows[0]["status"] != "completed" || fmt.Sprint(rows[0]["memory_mutation_attempts"]) != "1" {
		t.Fatalf("unexpected ledger rows: %#v", rows)
	}
	for _, forbidden := range []string{"output", "prompt", "content", "body"} {
		if _, exists := rows[0][forbidden]; exists {
			t.Fatalf("ledger persisted forbidden narrative field %q", forbidden)
		}
	}
}

func TestRunLedgerPreservesConcurrentRunsFromIndependentWriters(t *testing.T) {
	projectDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o600); err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()
	ledgers := make([]*runLedger, 2)
	errs := make([]error, 2)
	var wg sync.WaitGroup
	for i := range ledgers {
		wg.Add(1)
		go func(index int) {
			defer wg.Done()
			ledgers[index], errs[index] = openRunLedger(ctx, projectDir)
		}(i)
	}
	wg.Wait()
	for i, err := range errs {
		if err != nil {
			t.Fatalf("opening concurrent ledger %d: %v", i, err)
		}
		defer func(ledger *runLedger) { _ = ledger.Close() }(ledgers[i])
	}

	runIDs := []string{generateDreamID(), generateDreamID()}
	for i := range ledgers {
		wg.Add(1)
		go func(index int) {
			defer wg.Done()
			record := newRunRecord(projectDir, runIDs[index], "agent", "codex", time.Now())
			record.Status = "completed"
			errs[index] = ledgers[index].Put(ctx, record)
		}(i)
	}
	wg.Wait()
	for i, err := range errs {
		if err != nil {
			t.Fatalf("writing concurrent ledger %d: %v", i, err)
		}
	}

	verifier, err := openRunLedger(ctx, projectDir)
	if err != nil {
		t.Fatalf("opening verifier ledger: %v", err)
	}
	defer func() { _ = verifier.Close() }()
	rows, err := verifier.table.Rows(ctx)
	if err != nil {
		t.Fatal(err)
	}
	found := map[string]bool{}
	for _, row := range rows {
		found[rowString(row, "run_id")] = true
	}
	for _, runID := range runIDs {
		if !found[runID] {
			t.Fatalf("concurrent run %s was lost; rows: %#v", runID, rows)
		}
	}
}
