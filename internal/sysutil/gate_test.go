package sysutil

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestHeavySlotsDefaultsToOne(t *testing.T) {
	t.Setenv(heavySlotsEnv, "")
	if got := HeavySlots(); got != 1 {
		t.Errorf("HeavySlots() = %d, want 1", got)
	}
}

func TestHeavySlotsEnvOverrideIsCappedByCPUBudget(t *testing.T) {
	t.Setenv(heavySlotsEnv, "3")
	if got, want := HeavySlots(), min(3, CPUBudget()); got != want {
		t.Errorf("HeavySlots() = %d, want %d", got, want)
	}

	t.Setenv(heavySlotsEnv, "100000")
	if got, want := HeavySlots(), CPUBudget(); got != want {
		t.Errorf("HeavySlots() with an absurd override = %d, want the CPU budget %d", got, want)
	}
}

// The whole point of the gate: with one slot, a second pipeline waits instead of
// claiming a second copy of a budget sized for one.
func TestAcquireHeavySerializesWithOneSlot(t *testing.T) {
	t.Setenv(heavySlotsEnv, "1")
	resetHeavyGate()
	t.Cleanup(resetHeavyGate)

	var inFlight, peak atomic.Int64
	var wg sync.WaitGroup

	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			release, err := AcquireHeavy(context.Background())
			if err != nil {
				t.Errorf("AcquireHeavy: %v", err)
				return
			}
			defer release()

			n := inFlight.Add(1)
			for {
				old := peak.Load()
				if n <= old || peak.CompareAndSwap(old, n) {
					break
				}
			}
			time.Sleep(time.Millisecond)
			inFlight.Add(-1)
		}()
	}
	wg.Wait()

	if peak.Load() != 1 {
		t.Errorf("peak concurrency = %d, want 1", peak.Load())
	}
}

func TestAcquireHeavyReturnsTheSlotOnRelease(t *testing.T) {
	t.Setenv(heavySlotsEnv, "1")
	resetHeavyGate()
	t.Cleanup(resetHeavyGate)

	release, err := AcquireHeavy(context.Background())
	if err != nil {
		t.Fatalf("AcquireHeavy: %v", err)
	}
	release()
	release() // idempotent: a deferred release next to an explicit one must not free twice

	second, err := AcquireHeavy(context.Background())
	if err != nil {
		t.Fatalf("AcquireHeavy after release: %v", err)
	}
	second()
}

// A parked supervisor must stop queueing for work nobody is waiting on.
func TestAcquireHeavyAbandonsTheQueueOnCancel(t *testing.T) {
	t.Setenv(heavySlotsEnv, "1")
	resetHeavyGate()
	t.Cleanup(resetHeavyGate)

	held, err := AcquireHeavy(context.Background())
	if err != nil {
		t.Fatalf("AcquireHeavy: %v", err)
	}
	defer held()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	release, err := AcquireHeavy(ctx)
	if !errors.Is(err, context.Canceled) {
		t.Errorf("AcquireHeavy on a cancelled context: err = %v, want context.Canceled", err)
	}
	if release != nil {
		t.Error("AcquireHeavy returned a release func alongside an error")
	}
}

// The case above holds the only slot, so the cancelled context is the only ready
// case in the select. With the slot FREE both cases are ready, and Go picks a ready
// case at random — so a cancelled acquire succeeded about half the time, and a
// supervisor being parked started a full reindex on its way out. A free slot is the
// common case, which is what made this present as a flaky test in internal/daemon
// rather than as the race it is.
//
// Repeated because one iteration of a 50/50 race proves nothing.
func TestAcquireHeavyRefusesACancelledContextEvenWhenASlotIsFree(t *testing.T) {
	t.Setenv(heavySlotsEnv, "1")
	resetHeavyGate()
	t.Cleanup(resetHeavyGate)

	for i := 0; i < 200; i++ {
		ctx, cancel := context.WithCancel(context.Background())
		cancel()

		release, err := AcquireHeavy(ctx)
		if release != nil {
			release()
			t.Fatalf("attempt %d: acquired a slot on a cancelled context — the caller "+
				"would go on to do the work it was cancelled out of", i)
		}
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("attempt %d: err = %v, want context.Canceled", i, err)
		}
	}

	// And the slot was never consumed by those refusals.
	release, err := AcquireHeavy(context.Background())
	if err != nil {
		t.Fatalf("the gate leaked its slot across refused acquires: %v", err)
	}
	release()
}
