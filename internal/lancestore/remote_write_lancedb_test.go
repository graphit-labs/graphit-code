//go:build lancedb

package lancestore

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"testing"
	"time"
)

// A PUBLISHED ARTIFACT STAYS IMMUTABLE. This is the property the writable flag must not cost, so
// it is asserted first: the default for a remote URI is still that every write is refused.
func TestARemoteStoreRefusesEveryWriteUnlessTheCallerAsked(t *testing.T) {
	ctx := context.Background()
	cfg := remoteConfig(t, "immutable")

	writable, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("opening writable: %v", err)
	}
	defer func() { _ = writable.Close() }()
	if _, err := writable.CreateTable(ctx, "seeded", testSchema()); err != nil {
		t.Fatalf("a writable remote store must accept a create: %v", err)
	}

	cfg.Writable = false
	consumer, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("opening read-only: %v", err)
	}
	defer func() { _ = consumer.Close() }()

	if !consumer.Remote() {
		t.Error("Remote() must stay true for an s3:// store — maintenance decisions read it")
	}
	if !consumer.ReadOnly() {
		t.Fatal("a remote store with no stated write intent must be read-only")
	}

	tbl, err := consumer.OpenTable(ctx, "seeded")
	if err != nil {
		t.Fatalf("a read-only store must still OPEN a table: %v", err)
	}

	writes := map[string]func() error{
		"CreateTable":    func() error { _, e := consumer.CreateTable(ctx, "other", testSchema()); return e },
		"DropTable":      func() error { return consumer.DropTable(ctx, "seeded") },
		"Append":         func() error { return tbl.Append(ctx, testRows) },
		"DeleteWhere":    func() error { return tbl.DeleteWhere(ctx, "true") },
		"DeleteByKey":    func() error { return tbl.DeleteByKey(ctx, "uid", []string{"u1"}) },
		"Upsert":         func() error { return tbl.Upsert(ctx, "uid", testRows) },
		"EnsureIndexes":  func() error { return tbl.EnsureIndexes(ctx, Index{Column: "body", Kind: IndexInvertedText}) },
		"Compact":        func() error { _, e := tbl.Compact(ctx); return e },
		"PruneVersions":  func() error { _, e := tbl.PruneVersions(ctx, time.Hour); return e },
		"FoldNewRows":    func() error { return tbl.FoldNewRowsIntoIndexes(ctx) },
		"RestoreVersion": func() error { return tbl.RestoreVersion(ctx, 1) },
	}
	for name, write := range writes {
		if err := write(); !errors.Is(err, ErrReadOnly) {
			t.Errorf("%s on a read-only store = %v; want ErrReadOnly", name, err)
		}
	}

	if _, err := tbl.Count(ctx); err != nil {
		t.Errorf("a read-only store must still read: %v", err)
	}
}

// TWO WRITERS, ONE REMOTE TABLE. The rows of both have to survive, which is what a shared memory
// scope needs and what a lost commit would break.
//
// It also PRINTS every error the retry loop classified, because the classifier matches on message
// text and there is nothing else to match on — the binding surfaces engine failures as plain
// errors with no code. The phrases in commitConflictPhrases have to come from what this observed,
// not from a guess at the wording.
func TestTwoWritersRaceOnOneRemoteTableAndBothCommit(t *testing.T) {
	ctx := context.Background()
	cfg := remoteConfig(t, "race")

	first, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("opening the first writer: %v", err)
	}
	defer func() { _ = first.Close() }()
	if _, err := first.CreateTable(ctx, "memories", testSchema()); err != nil {
		t.Fatalf("creating the table: %v", err)
	}

	second, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("opening the second writer: %v", err)
	}
	defer func() { _ = second.Close() }()

	const roundsPerWriter = 12
	start := make(chan struct{})
	writer := func(store *Store, tag string) []error {
		tbl, openErr := store.OpenTable(ctx, "memories")
		if openErr != nil {
			return []error{fmt.Errorf("%s: opening the table: %w", tag, openErr)}
		}
		var errs []error
		<-start
		for i := 0; i < roundsPerWriter; i++ {
			row := Row{
				"uid":           fmt.Sprintf("%s-%d", tag, i),
				"path":          tag + ".md",
				"name":          tag,
				"body":          fmt.Sprintf("written by %s in round %d", tag, i),
				"line":          int64(i),
				"is_dependency": false,
			}
			if err := tbl.Append(ctx, []Row{row}); err != nil {
				errs = append(errs, fmt.Errorf("%s round %d: %w", tag, i, err))
			}
		}
		return errs
	}

	conflictsBefore := CommitConflictsRetried()

	var wg sync.WaitGroup
	results := make([][]error, 2)
	for i, spec := range []struct {
		store *Store
		tag   string
	}{{first, "alpha"}, {second, "beta"}} {
		wg.Add(1)
		go func(idx int, store *Store, tag string) {
			defer wg.Done()
			results[idx] = writer(store, tag)
		}(i, spec.store, spec.tag)
	}
	close(start)
	wg.Wait()

	for _, errs := range results {
		for _, err := range errs {
			t.Errorf("a concurrent write failed: %v (classified as a commit conflict: %v)",
				err, isCommitConflict(err))
		}
	}

	retried := CommitConflictsRetried() - conflictsBefore
	t.Logf("commits retried after losing a race: %d (of %d concurrent appends)",
		retried, 2*roundsPerWriter)

	reader, err := Open(ctx, Config{URI: cfg.URI, S3: cfg.S3})
	if err != nil {
		t.Fatalf("opening a reader: %v", err)
	}
	defer func() { _ = reader.Close() }()
	tbl, err := reader.OpenTable(ctx, "memories")
	if err != nil {
		t.Fatalf("opening the table to read: %v", err)
	}
	n, err := tbl.Count(ctx)
	if err != nil {
		t.Fatalf("counting: %v", err)
	}
	if want := int64(2 * roundsPerWriter); n != want {
		t.Errorf("rows on s3 = %d, want %d — a commit was lost", n, want)
	}

	for _, tag := range []string{"alpha", "beta"} {
		hits, err := tbl.Search(ctx, Query{Filter: fmt.Sprintf("name = '%s'", tag), Limit: 100})
		if err != nil {
			t.Fatalf("reading %s's rows: %v", tag, err)
		}
		if len(hits) != roundsPerWriter {
			t.Errorf("%s wrote %d rows, want %d", tag, len(hits), roundsPerWriter)
		}
	}
}

func TestAnEarlierVersionIsRestorableAfterADestructiveWrite(t *testing.T) {
	ctx := context.Background()
	_, tbl := remoteTable(t, "rollback")

	if err := tbl.Append(ctx, testRows); err != nil {
		t.Fatalf("seeding: %v", err)
	}
	good, err := tbl.CurrentVersion(ctx)
	if err != nil {
		t.Fatalf("reading the version: %v", err)
	}
	before, err := tbl.Count(ctx)
	if err != nil {
		t.Fatalf("counting: %v", err)
	}
	if before != int64(len(testRows)) {
		t.Fatalf("seeded %d rows, want %d", before, len(testRows))
	}

	if err := tbl.DeleteWhere(ctx, "true"); err != nil {
		t.Fatalf("the destructive write: %v", err)
	}
	if n, err := tbl.Count(ctx); err != nil || n != 0 {
		t.Fatalf("after deleting everything: count = %d, err = %v; want 0", n, err)
	}

	versions, err := tbl.Versions(ctx)
	if err != nil {
		t.Fatalf("listing versions: %v", err)
	}
	if len(versions) < 2 {
		t.Fatalf("history has %d version(s); a seed and a delete must produce at least two", len(versions))
	}
	if versions[0].Version <= good {
		t.Errorf("versions are not newest-first: %d then %d", versions[0].Version, good)
	}

	if err := tbl.RestoreVersion(ctx, good); err != nil {
		t.Fatalf("restoring version %d: %v", good, err)
	}

	after, err := tbl.Count(ctx)
	if err != nil {
		t.Fatalf("counting after the restore: %v", err)
	}
	if after != before {
		t.Fatalf("after restoring version %d: %d rows, want %d", good, after, before)
	}

	hits, err := tbl.Search(ctx, Query{Filter: "uid = 'u3'", Limit: 1})
	if err != nil {
		t.Fatalf("reading a restored row: %v", err)
	}
	if len(hits) != 1 || hits[0].Row["name"] != "ReciprocalRank" {
		t.Errorf("restored row = %v; want the seeded u3", hits)
	}

	if v, err := tbl.CurrentVersion(ctx); err != nil {
		t.Fatalf("reading the version after the restore: %v", err)
	} else if v <= good {
		t.Errorf("version after restore = %d; want a NEW version above %d", v, good)
	}

	if err := tbl.Append(ctx, []Row{{
		"uid": "post-restore", "path": "z.md", "name": "PostRestore",
		"body": "written after the rollback", "line": int64(1), "is_dependency": false,
	}}); err != nil {
		t.Errorf("writing after a restore: %v — the handle is still pinned to the snapshot", err)
	}
}

// CHECKOUT IS A READ, and it has to stay one: inspecting what an earlier version held must not
// promote it, or "let me look" becomes destructive.
func TestCheckoutInspectsAVersionWithoutPromotingIt(t *testing.T) {
	ctx := context.Background()
	_, tbl := remoteTable(t, "checkout")

	if err := tbl.Append(ctx, testRows[:2]); err != nil {
		t.Fatalf("seeding: %v", err)
	}
	early, err := tbl.CurrentVersion(ctx)
	if err != nil {
		t.Fatalf("reading the version: %v", err)
	}
	if err := tbl.Append(ctx, testRows[2:]); err != nil {
		t.Fatalf("second write: %v", err)
	}
	latest, err := tbl.Count(ctx)
	if err != nil {
		t.Fatalf("counting: %v", err)
	}

	if err := tbl.CheckoutVersion(ctx, early); err != nil {
		t.Fatalf("checking out %d: %v", early, err)
	}
	pinned, err := tbl.Count(ctx)
	if err != nil {
		t.Fatalf("counting the pinned snapshot: %v", err)
	}
	if pinned != 2 {
		t.Errorf("the pinned snapshot has %d rows, want 2", pinned)
	}

	if err := tbl.CheckoutLatest(ctx); err != nil {
		t.Fatalf("returning to latest: %v", err)
	}
	if n, err := tbl.Count(ctx); err != nil || n != latest {
		t.Errorf("after CheckoutLatest: %d rows (err %v), want %d — the checkout changed the data",
			n, err, latest)
	}
}

// THE RETRY LOOP'S REASON TO EXIST, proved on the operation that actually races.
//
// The plan expected the contention to be on the writes. It is not: concurrent appends, deletes and
// same-key upserts are resolved by Lance itself. Compaction is what preempts compaction —
// `This Rewrite transaction was preempted by concurrent transaction Rewrite at version N. Please
// retry.` — and before the retry wrapped it, three concurrent compactors failed six times out of
// nine.
//
// Two processes maintaining one table is the normal case here rather than an exotic one: the daemon
// runs maintenance per project, and a memory scope shared by several units would have one
// maintainer per unit.
func TestConcurrentCompactionConflictsAndTheRetryAbsorbsIt(t *testing.T) {
	ctx := context.Background()
	cfg := remoteConfig(t, "compaction-race")

	seedStore, err := Open(ctx, cfg)
	if err != nil {
		t.Fatalf("opening the seed store: %v", err)
	}
	defer func() { _ = seedStore.Close() }()
	seed, err := seedStore.CreateTable(ctx, "memories", testSchema())
	if err != nil {
		t.Fatalf("creating the table: %v", err)
	}
	for i := 0; i < 8; i++ {
		if err := seed.Append(ctx, testRows); err != nil {
			t.Fatalf("seeding round %d: %v", i, err)
		}
	}
	if err := seed.DeleteByKey(ctx, "uid", []string{"u4", "u5"}); err != nil {
		t.Fatalf("seeding tombstones: %v", err)
	}

	const compactors = 3
	const roundsEach = 3

	conflictsBefore := CommitConflictsRetried()
	start := make(chan struct{})
	var wg sync.WaitGroup
	failures := make(chan error, compactors*roundsEach)

	for c := 0; c < compactors; c++ {
		store, err := Open(ctx, cfg)
		if err != nil {
			t.Fatalf("opening compactor %d: %v", c, err)
		}
		defer func() { _ = store.Close() }()
		tbl, err := store.OpenTable(ctx, "memories")
		if err != nil {
			t.Fatalf("compactor %d opening the table: %v", c, err)
		}
		wg.Add(1)
		go func(tbl *Table, id int) {
			defer wg.Done()
			<-start
			for i := 0; i < roundsEach; i++ {
				if _, err := tbl.Compact(ctx); err != nil {
					failures <- fmt.Errorf("compactor %d round %d: %w", id, i, err)
				}
			}
		}(tbl, c)
	}
	close(start)
	wg.Wait()
	close(failures)

	for err := range failures {
		t.Errorf("a concurrent compaction failed: %v (classified as a commit conflict: %v)",
			err, isCommitConflict(err))
	}

	retried := CommitConflictsRetried() - conflictsBefore
	t.Logf("compaction commits retried: %d (of %d concurrent compactions)", retried, compactors*roundsEach)
	if retried == 0 {
		t.Log("no conflict arose in this run, so the retry loop was not exercised — " +
			"the assertion that holds is that concurrent compaction did not fail")
	}

	n, err := seed.Count(ctx)
	if err != nil {
		t.Fatalf("counting after the compactions: %v", err)
	}
	if want := int64(8 * (len(testRows) - 2)); n != want {
		t.Errorf("rows after concurrent compaction = %d, want %d", n, want)
	}
}

func TestVersionRetentionDecidesWhatAPruneReclaims(t *testing.T) {
	ctx := context.Background()
	_, tbl := remoteTable(t, "retention")

	for i := 0; i < 4; i++ {
		if err := tbl.Append(ctx, testRows[i:i+1]); err != nil {
			t.Fatalf("write %d: %v", i, err)
		}
	}
	history, err := tbl.Versions(ctx)
	if err != nil {
		t.Fatalf("listing versions: %v", err)
	}
	if len(history) < 4 {
		t.Fatalf("history has %d versions; four writes must produce at least four", len(history))
	}

	kept, err := tbl.PruneVersions(ctx, time.Hour)
	if err != nil {
		t.Fatalf("pruning with an hour of retention: %v", err)
	}
	if kept.OldVersions != 0 {
		t.Errorf("an hour of retention reclaimed %d version(s); a young version must survive", kept.OldVersions)
	}
	if after, err := tbl.Versions(ctx); err != nil {
		t.Fatalf("listing versions after the no-op prune: %v", err)
	} else if len(after) != len(history) {
		t.Errorf("history went from %d to %d versions under an hour of retention", len(history), len(after))
	}

	time.Sleep(1100 * time.Millisecond)
	reclaimed, err := tbl.PruneVersions(ctx, time.Second)
	if err != nil {
		t.Fatalf("pruning with a second of retention: %v", err)
	}
	if reclaimed.OldVersions == 0 {
		t.Errorf("a second of retention reclaimed nothing; the versions are older than that")
	}

	if n, err := tbl.Count(ctx); err != nil || n != 4 {
		t.Errorf("after pruning: %d rows (err %v), want 4", n, err)
	}

	if _, err := tbl.PruneVersions(ctx, 0); err == nil {
		t.Error("a zero retention must be refused, not treated as a window")
	}
}
