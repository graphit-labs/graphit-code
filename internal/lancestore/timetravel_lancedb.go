//go:build lancedb

package lancestore

import (
	"context"
	"fmt"
	"math/rand/v2"
	"strings"
	"sync/atomic"
	"time"

	"github.com/lancedb/lancedb-go/pkg/contracts"
)

func (t *Table) timeTravel() (contracts.ITableTimeTravel, error) {
	tt, ok := t.tbl.(contracts.ITableTimeTravel)
	if !ok {
		return nil, fmt.Errorf("lancestore: %s: %w", t.name, ErrNoTimeTravel)
	}
	return tt, nil
}

func (t *Table) Versions(ctx context.Context) ([]Version, error) {
	tt, err := t.timeTravel()
	if err != nil {
		return nil, err
	}
	infos, err := tt.ListVersions(ctx)
	if err != nil {
		return nil, fmt.Errorf("lancestore: listing versions of %s: %w", t.name, err)
	}
	out := make([]Version, 0, len(infos))
	for _, info := range infos {
		out = append(out, Version{
			Version:   info.Version,
			Timestamp: info.Timestamp,
			Metadata:  info.Metadata,
		})
	}
	sortVersionsNewestFirst(out)
	return out, nil
}

// CurrentVersion is the version the table is reading now.
func (t *Table) CurrentVersion(ctx context.Context) (uint64, error) {
	v, err := t.tbl.Version(ctx)
	if err != nil {
		return 0, fmt.Errorf("lancestore: reading the version of %s: %w", t.name, err)
	}
	if v < 0 {
		return 0, fmt.Errorf("lancestore: %s reported a negative version %d", t.name, v)
	}
	return uint64(v), nil
}

// CheckoutVersion pins the table to a past version, so reads see that snapshot.
//
// WRITES ARE REJECTED WHILE PINNED, by the engine, until CheckoutLatest drops the pin or Restore
// promotes the snapshot. That is why this is exposed separately from RestoreVersion: inspecting
// what an earlier version held is a read-only act, and conflating it with promoting that version
// would make "let me look" destructive.
func (t *Table) CheckoutVersion(ctx context.Context, version uint64) error {
	tt, err := t.timeTravel()
	if err != nil {
		return err
	}
	if err := tt.Checkout(ctx, version); err != nil {
		return fmt.Errorf("lancestore: checking out version %d of %s: %w", version, t.name, err)
	}
	return nil
}

// CheckoutLatest drops any pin and resumes tracking the newest version.
func (t *Table) CheckoutLatest(ctx context.Context) error {
	tt, err := t.timeTravel()
	if err != nil {
		return err
	}
	if err := tt.CheckoutLatest(ctx); err != nil {
		return fmt.Errorf("lancestore: returning %s to its latest version: %w", t.name, err)
	}
	return nil
}

func (t *Table) RestoreVersion(ctx context.Context, version uint64) error {
	if t.store.readOnly {
		return ErrReadOnly
	}
	tt, err := t.timeTravel()
	if err != nil {
		return err
	}
	if err := tt.Checkout(ctx, version); err != nil {
		return fmt.Errorf("lancestore: checking out version %d of %s: %w", version, t.name, err)
	}
	if err := tt.Restore(ctx); err != nil {
		if latestErr := tt.CheckoutLatest(ctx); latestErr != nil {
			return fmt.Errorf("lancestore: restoring version %d of %s failed (%w) and the table "+
				"is still pinned to it (%v) — it will reject writes until reopened", version, t.name, err, latestErr)
		}
		return fmt.Errorf("lancestore: promoting version %d of %s: %w", version, t.name, err)
	}
	return nil
}

// PutTag creates tag or moves it to version when it already exists.
func (t *Table) PutTag(ctx context.Context, tag string, version uint64) error {
	if t.store.readOnly {
		return ErrReadOnly
	}
	tt, err := t.timeTravel()
	if err != nil {
		return err
	}
	if _, exists := func() (uint64, bool) {
		v, getErr := tt.TagGetVersion(ctx, tag)
		return v, getErr == nil
	}(); exists {
		if err := tt.TagUpdate(ctx, tag, version); err != nil {
			return fmt.Errorf("lancestore: updating tag %s on %s: %w", tag, t.name, err)
		}
		return nil
	}
	if err := tt.TagCreate(ctx, tag, version); err != nil {
		return fmt.Errorf("lancestore: creating tag %s on %s: %w", tag, t.name, err)
	}
	return nil
}

func sortVersionsNewestFirst(vs []Version) {
	for i := 1; i < len(vs); i++ {
		for j := i; j > 0 && vs[j].Version > vs[j-1].Version; j-- {
			vs[j], vs[j-1] = vs[j-1], vs[j]
		}
	}
}

const commitRetries = 6

// commitRetryBase is the first backoff. Each attempt doubles it and adds jitter, so two writers
// that collide do not then collide again in lockstep.
const commitRetryBase = 40 * time.Millisecond

// commitConflictsRetried counts the commits this process retried after losing a race.
//
// It exists because a retry loop that never runs is indistinguishable from one that works, and a
// concurrency test that passes without ever contending proves nothing. A test reads it to say
// which of the two happened; production reads it to report contention rather than guess at it.
var commitConflictsRetried atomic.Int64

// CommitConflictsRetried is how many commits have been retried after losing a race.
func CommitConflictsRetried() int64 { return commitConflictsRetried.Load() }

// withCommitRetry runs a mutating operation, retrying while the failure is a lost commit race.
//
// It wraps the write rather than living inside each method so that the classifier and the backoff
// exist once. A non-conflict error is returned immediately and untouched: retrying a schema
// mismatch or a credentials failure just delays the report.
//
// 🔒 `beforeRetry` IS NOT OPTIONAL DECORATION, and this is the part that was learned the hard way.
// A first version of this loop retried the operation unchanged, and three concurrent compactors
// then failed SIX times out of nine having burned 42 retries between them — every one of them
// conflicting on the same version number, over and over. The reason: a Rewrite transaction is
// computed against the version the table handle is sitting on. The winner moves the table forward;
// the losers keep re-submitting a rewrite of a version that is no longer the tip, so the conflict
// is not transient, it is permanent, and more attempts cannot help.
//
// So a retry has to bring the handle back to the latest manifest first. A retry without that is not
// a weaker retry, it is a busy-wait that cannot succeed — which is a far worse failure than not
// retrying at all, because it looks like resilience.
func withCommitRetry(ctx context.Context, what string, fn func() error, beforeRetry func() error) error {
	var lastErr error
	for attempt := 0; attempt <= commitRetries; attempt++ {
		lastErr = fn()
		if lastErr == nil {
			return nil
		}
		if !isCommitConflict(lastErr) {
			return lastErr
		}
		commitConflictsRetried.Add(1)
		if attempt == commitRetries {
			break
		}
		delay := commitRetryBase << attempt
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(delay + jitter(delay)):
		}
		if beforeRetry != nil {
			if err := beforeRetry(); err != nil {
				return fmt.Errorf("lancestore: %s: preparing to retry after a commit conflict: %w", what, err)
			}
		}
	}
	return fmt.Errorf("lancestore: %s after %d attempts: %w: %w",
		what, commitRetries+1, ErrCommitConflict, lastErr)
}

// refreshToLatest brings the table handle back to the newest manifest, so a retried write is
// computed against the version that is actually the tip.
//
// A table with no time-travel capability cannot be refreshed, and that is NOT an error here: the
// retry is still worth attempting, it just cannot fix a stale-base conflict. Reporting it as a
// failure would turn a survivable conflict into a returned error on backends that never had the
// problem.
func (t *Table) refreshToLatest(ctx context.Context) error {
	tt, ok := t.tbl.(contracts.ITableTimeTravel)
	if !ok {
		return nil
	}
	if err := tt.CheckoutLatest(ctx); err != nil {
		return fmt.Errorf("returning %s to its latest version: %w", t.name, err)
	}
	return nil
}

// Refresh advances this table handle to the latest committed manifest.
// Shared coordination code calls it after acquiring its scheduler lease so a
// handle opened while another writer owned the lease cannot make a decision
// from the older snapshot.
func (t *Table) Refresh(ctx context.Context) error {
	return t.refreshToLatest(ctx)
}

// isCommitConflict says whether a failed write lost a commit race, as opposed to being wrong.
//
// IT MATCHES ON THE MESSAGE, and that is a real weakness worth stating rather than hiding: the
// binding surfaces engine failures as plain errors with no code and no sentinel to compare
// against, so there is nothing else to match on. The consequence to keep in mind is that an
// upstream rewording turns a retryable conflict into a returned error — the write fails loudly
// rather than silently, which is the safe direction to fail in.
func isCommitConflict(err error) bool {
	if err == nil {
		return false
	}
	msg := strings.ToLower(err.Error())
	for _, phrase := range commitConflictPhrases {
		if strings.Contains(msg, phrase) {
			return true
		}
	}
	return false
}

var commitConflictPhrases = []string{
	"commit conflict",
	"preempted by concurrent",
	"please retry",
}

// jitter spreads retries so two writers that collided do not collide again in step.
//
// Deterministic sources were considered and rejected: the whole purpose is that two processes
// which are otherwise identical pick different delays, and anything derived from the operation
// would give them the same answer.
func jitter(d time.Duration) time.Duration {
	if d <= 0 {
		return 0
	}
	return time.Duration(rand.Int64N(int64(d) / 2))
}
