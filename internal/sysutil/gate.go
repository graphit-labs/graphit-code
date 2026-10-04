package sysutil

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"path/filepath"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

const heavySlotsEnv = "GRAPHIT_HEAVY_SLOTS"

// heavyPollInterval is how often a waiting process retries the cross-process slots.
const heavyPollInterval = 500 * time.Millisecond

var (
	heavyOnce sync.Once
	heavyGate chan struct{}
)

// HeavySlots reports how many CPU-saturating jobs may run at once on this machine.
//
// The default is 1 by construction, not by conservatism: CPUBudget already hands a
// single pipeline as much of the machine as it may have, so a second concurrent slot
// is by definition oversubscription. GRAPHIT_HEAVY_SLOTS raises it (clamped to the CPU
// budget) for an operator who would rather trade peak memory for throughput.
func HeavySlots() int {
	n := 1
	if v := envInt(heavySlotsEnv); v > 0 {
		n = v
	}
	if b := CPUBudget(); n > b {
		n = b
	}
	return n
}

func heavyChan() chan struct{} {
	heavyOnce.Do(func() { heavyGate = make(chan struct{}, HeavySlots()) })
	return heavyGate
}

// heavySlotPath is the lock file backing cross-process slot i.
func heavySlotPath(i int) string {
	return filepath.Join(brand.GlobalDir(), "locks", fmt.Sprintf("heavy-slot-%d.lock", i))
}

// AcquireHeavy waits for a heavy-work slot and returns the function that gives it
// back. The release function is idempotent, so `defer release()` stays safe next to an
// earlier explicit call.
//
// The slots are shared by every process on the machine: indexing and embedding run as
// separate CLI processes, so a per-process semaphore alone would let each of them claim
// the whole CPU budget at once. A slot is an advisory file lock, which the kernel drops
// when its holder exits however it exits. Only the processes doing the heavy work take
// a slot — a parent waiting on such a child must not hold one, or the child would wait
// for its own parent.
//
// A cancelled wait returns ctx.Err() with a nil release, and the caller MUST skip the
// work: a supervisor being parked or a daemon shutting down should not keep queueing
// for a slot to do work nobody is waiting on any more.
func AcquireHeavy(ctx context.Context) (release func(), err error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	select {
	case heavyChan() <- struct{}{}:
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	local := func() { <-heavyChan() }

	lock, err := acquireHeavySlot(ctx, HeavySlots())
	if err != nil {
		local()
		return nil, err
	}

	var once sync.Once
	return func() {
		once.Do(func() {
			lock.Release()
			local()
		})
	}, nil
}

// acquireHeavySlot takes any free cross-process slot, polling until one frees up. A slot
// directory that cannot be used degrades to the in-process gate alone (nil lock) rather
// than stopping the work: losing machine-wide serialization costs throughput, refusing
// to index costs correctness.
func acquireHeavySlot(ctx context.Context, slots int) (*lockfile.Lock, error) {
	var logged bool
	for {
		for i := 0; i < slots; i++ {
			lock, err := lockfile.TryAcquire(heavySlotPath(i))
			if err == nil {
				return lock, nil
			}
			if !errors.Is(err, lockfile.ErrLocked) {
				slog.Warn("heavy-work slot unavailable; using the in-process gate only", "error", err)
				return nil, nil
			}
		}
		if !logged {
			slog.Info("waiting for a machine-wide heavy-work slot", "slots", slots)
			logged = true
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(heavyPollInterval):
		}
	}
}

func resetHeavyGate() {
	heavyOnce = sync.Once{}
	heavyGate = nil
}
