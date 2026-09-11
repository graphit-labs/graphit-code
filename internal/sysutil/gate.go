package sysutil

import (
	"context"
	"sync"
)

const heavySlotsEnv = "GRAPHIT_HEAVY_SLOTS"

var (
	heavyOnce sync.Once
	heavyGate chan struct{}
)

// HeavySlots reports how many CPU-saturating jobs may run at once in this process.
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

// AcquireHeavy waits for a heavy-work slot and returns the function that gives it
// back. The release function is idempotent, so `defer release()` stays safe next to an
// earlier explicit call.
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

	var once sync.Once
	return func() { once.Do(func() { <-heavyChan() }) }, nil
}

func resetHeavyGate() {
	heavyOnce = sync.Once{}
	heavyGate = nil
}
