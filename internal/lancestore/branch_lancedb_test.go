//go:build lancedb

package lancestore

import (
	"context"
	"sort"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

var branchItems = Schema{Fields: []Field{
	{Name: "id", Type: FieldString},
	{Name: "value", Type: FieldString},
}}

func openBranchStore(t *testing.T, cfg Config) *Store {
	t.Helper()
	s, err := Open(context.Background(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s
}

func ids(t *testing.T, s *Store) []string {
	t.Helper()
	table, err := s.OpenTable(context.Background(), "items")
	if err != nil {
		t.Fatal(err)
	}
	if err := table.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	rows, err := table.Rows(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	out := make([]string, 0, len(rows))
	for _, row := range rows {
		out = append(out, row["id"].(string))
	}
	sort.Strings(out)
	return out
}

func appendIDs(t *testing.T, s *Store, values ...string) {
	t.Helper()
	table, err := s.EnsureTable(context.Background(), "items", branchItems)
	if err != nil {
		t.Fatal(err)
	}
	rows := make([]Row, 0, len(values))
	for _, v := range values {
		rows = append(rows, Row{"id": v, "value": v})
	}
	if err := table.Append(context.Background(), rows); err != nil {
		t.Fatal(err)
	}
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// A branch starts from the main lineage as it is when the branch is created, and the two
// diverge afterwards in both directions.
func TestBranchInheritsMainAndDivergesBothWays(t *testing.T) {
	dir := t.TempDir()
	main := openBranchStore(t, Config{URI: dir, Writable: true})
	appendIDs(t, main, "a", "b")

	feature := openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/x"})
	if got := ids(t, feature); !equal(got, []string{"a", "b"}) {
		t.Fatalf("the branch did not inherit the main lineage: %v", got)
	}
	appendIDs(t, feature, "only-feature")
	appendIDs(t, main, "only-main")

	if got := ids(t, feature); !equal(got, []string{"a", "b", "only-feature"}) {
		t.Fatalf("feature = %v", got)
	}
	if got := ids(t, main); !equal(got, []string{"a", "b", "only-main"}) {
		t.Fatalf("main = %v", got)
	}

	// Reopening finds the branch again rather than forking a new one.
	again := openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/x"})
	if got := ids(t, again); !equal(got, []string{"a", "b", "only-feature"}) {
		t.Fatalf("reopened feature = %v", got)
	}
	branches, err := main.Branches(context.Background(), "items")
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := branches["feature/x"]; !ok || len(branches) != 1 {
		t.Fatalf("branches = %#v", branches)
	}
}

// A store whose table does not exist yet creates it, empty, on the main lineage and
// forks the branch from it.
func TestEnsureTableOnABranchOfAnEmptyStore(t *testing.T) {
	dir := t.TempDir()
	feature := openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/y"})
	appendIDs(t, feature, "f")
	if got := ids(t, feature); !equal(got, []string{"f"}) {
		t.Fatalf("feature = %v", got)
	}
	main := openBranchStore(t, Config{URI: dir, Writable: true})
	if got := ids(t, main); len(got) != 0 {
		t.Fatalf("the branch's write reached the main lineage: %v", got)
	}
}

// Maintenance of the main lineage must not take away fragments a branch still inherits.
func TestMainLineageMaintenanceKeepsBranchData(t *testing.T) {
	dir := t.TempDir()
	main := openBranchStore(t, Config{URI: dir, Writable: true})
	for _, id := range []string{"a", "b", "c"} {
		appendIDs(t, main, id)
	}
	feature := openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/z"})
	if got := ids(t, feature); !equal(got, []string{"a", "b", "c"}) {
		t.Fatalf("feature = %v", got)
	}
	appendIDs(t, main, "d")
	table, err := main.OpenTable(context.Background(), "items")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := table.Compact(context.Background()); err != nil {
		t.Fatal(err)
	}
	time.Sleep(1200 * time.Millisecond)
	pruned, err := table.PruneVersions(context.Background(), time.Second)
	if err != nil {
		t.Fatal(err)
	}
	if pruned.OldVersions == 0 {
		t.Fatalf("pruning removed nothing, so it proves nothing: %#v", pruned)
	}
	reopened := openBranchStore(t, Config{URI: dir, Writable: true, Branch: "feature/z"})
	if got := ids(t, reopened); !equal(got, []string{"a", "b", "c"}) {
		t.Fatalf("after main compaction and pruning the branch reads %v", got)
	}
}

// A read-only store never creates a branch: until the version has written something it
// reads the main lineage it will fork from.
func TestReadOnlyStoreReadsMainUntilTheBranchExists(t *testing.T) {
	ctx := context.Background()
	_, endpoint := testsupport.StartFakeS3(t, "graphit-branches")
	s3 := config.S3Config{Bucket: "graphit-branches", Region: "us-east-1", Endpoint: endpoint,
		AccessKeyID: "test-key", SecretAccessKey: "test-secret"}
	uri := "s3://graphit-branches/memory"
	main := openBranchStore(t, Config{URI: uri, S3: s3, Writable: true})
	appendIDs(t, main, "a")

	reader := openBranchStore(t, Config{URI: uri, S3: s3, Branch: "feature/r"})
	if got := ids(t, reader); !equal(got, []string{"a"}) {
		t.Fatalf("read-only branch reader = %v", got)
	}
	branches, err := main.Branches(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if len(branches) != 0 {
		t.Fatalf("a read created a branch: %#v", branches)
	}

	writer := openBranchStore(t, Config{URI: uri, S3: s3, Writable: true, Branch: "feature/r"})
	appendIDs(t, writer, "r")
	reader2 := openBranchStore(t, Config{URI: uri, S3: s3, Branch: "feature/r"})
	if got := ids(t, reader2); !equal(got, []string{"a", "r"}) {
		t.Fatalf("read-only reader of an existing S3 branch = %v", got)
	}
	if got := ids(t, main); !equal(got, []string{"a"}) {
		t.Fatalf("S3 main lineage = %v", got)
	}
}
