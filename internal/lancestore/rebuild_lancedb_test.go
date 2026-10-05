//go:build lancedb

package lancestore

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"testing"
)

var grownItems = Schema{Fields: []Field{
	{Name: "id", Type: FieldString},
	{Name: "value", Type: FieldString},
	{Name: "extra", Type: FieldString, Nullable: true},
}}

func idsOf(t *testing.T, cfg Config) []string {
	t.Helper()
	return ids(t, openBranchStore(t, cfg))
}

// A rebuild keeps every row of the main lineage and of every branch, in the new schema.
func TestRebuildKeepsMainAndBranchRows(t *testing.T) {
	ctx := context.Background()
	dir, backups := t.TempDir(), t.TempDir()
	main := openBranchStore(t, Config{URI: dir, Writable: true})
	appendIDs(t, main, "a", "b")
	feature := openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/r"})
	appendIDs(t, feature, "f")
	_ = main.Close()
	_ = feature.Close()

	err := RebuildTable(ctx, Config{URI: dir}, RebuildPlan{Table: "items", Schema: grownItems, BranchKey: []string{"id"}, BackupDir: backups})
	if err != nil {
		t.Fatal(err)
	}
	if got := idsOf(t, Config{URI: dir, Writable: true}); !equal(got, []string{"a", "b"}) {
		t.Fatalf("main after rebuild = %v", got)
	}
	if got := idsOf(t, Config{URI: dir, Writable: true, Branch: "feature/r"}); !equal(got, []string{"a", "b", "f"}) {
		t.Fatalf("branch after rebuild = %v", got)
	}
	st := openBranchStore(t, Config{URI: dir, Writable: true})
	table, err := st.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if !table.Schema().Equal(grownItems) {
		t.Fatalf("schema after rebuild = %#v", table.Schema())
	}
	done, _ := filepath.Glob(filepath.Join(backups, "*.done.json"))
	if len(done) != 1 {
		t.Fatalf("the snapshot was not kept as a backup: %v", done)
	}
}

// A table with branches is not rebuilt without a key to replay them by.
func TestRebuildRefusesBranchesWithoutAKey(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	appendIDs(t, openBranchStore(t, Config{URI: dir, Writable: true}), "a")
	appendIDs(t, openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/k"}), "k")
	err := RebuildTable(ctx, Config{URI: dir}, RebuildPlan{Table: "items", Schema: grownItems, BackupDir: t.TempDir()})
	if !errors.Is(err, ErrRebuildRefused) {
		t.Fatalf("err = %v, want ErrRebuildRefused", err)
	}
	if got := idsOf(t, Config{URI: dir, Writable: true, Branch: "feature/k"}); !equal(got, []string{"a", "k"}) {
		t.Fatalf("the refused rebuild touched the branch: %v", got)
	}
}

// A table that cannot be read in full is never dropped, and neither is one whose target
// schema is invalid — the failure that once wiped memory tables when the embedding width
// resolved to zero.
func TestRebuildNeverDropsWhatItCannotSave(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	st := openBranchStore(t, Config{URI: dir, Writable: true})
	appendIDs(t, st, "a")
	appendIDs(t, st, "b")
	_ = st.Close()

	invalid := Schema{Fields: []Field{{Name: "id", Type: FieldString}, {Name: "v", Type: FieldVector, Dim: 0}}}
	if err := RebuildTable(ctx, Config{URI: dir}, RebuildPlan{Table: "items", Schema: invalid, BackupDir: t.TempDir()}); !errors.Is(err, ErrRebuildRefused) {
		t.Fatalf("an invalid target schema: err = %v", err)
	}
	if got := idsOf(t, Config{URI: dir, Writable: true}); !equal(got, []string{"a", "b"}) {
		t.Fatalf("the refused rebuild touched the table: %v", got)
	}

	fragments, _ := filepath.Glob(filepath.Join(dir, "items.lance", "data", "*.lance"))
	sort.Strings(fragments)
	if len(fragments) < 2 {
		t.Fatalf("expected two fragments, have %v", fragments)
	}
	if err := os.Remove(fragments[0]); err != nil {
		t.Fatal(err)
	}
	err := RebuildTable(ctx, Config{URI: dir}, RebuildPlan{Table: "items", Schema: grownItems, BackupDir: t.TempDir()})
	if !errors.Is(err, ErrRebuildRefused) {
		t.Fatalf("an unreadable table: err = %v, want ErrRebuildRefused", err)
	}
	if _, err := os.Stat(filepath.Join(dir, "items.lance", "_versions")); err != nil {
		t.Fatalf("the unreadable table was dropped: %v", err)
	}
}

// A rebuild interrupted after the drop is completed from its snapshot on the next open.
func TestRecoverRebuildCompletesAnInterruptedRebuild(t *testing.T) {
	ctx := context.Background()
	dir, backups := t.TempDir(), t.TempDir()
	st := openBranchStore(t, Config{URI: dir, Writable: true})
	appendIDs(t, st, "a", "b")
	snapshot, err := readRebuildSnapshot(ctx, Config{URI: dir, Writable: true}, RebuildPlan{Table: "items", Schema: grownItems})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := writeRebuildSnapshot(backups, snapshot); err != nil {
		t.Fatal(err)
	}
	if err := st.DropTable(ctx, "items"); err != nil { // the process dies here
		t.Fatal(err)
	}
	_ = st.Close()

	recovered, err := RecoverRebuild(ctx, Config{URI: dir}, "items", backups)
	if err != nil || !recovered {
		t.Fatalf("recovered=%v err=%v", recovered, err)
	}
	if got := idsOf(t, Config{URI: dir, Writable: true}); !equal(got, []string{"a", "b"}) {
		t.Fatalf("rows after recovery = %v", got)
	}
	if again, err := RecoverRebuild(ctx, Config{URI: dir}, "items", backups); err != nil || again {
		t.Fatalf("a completed recovery ran again: %v %v", again, err)
	}
}
