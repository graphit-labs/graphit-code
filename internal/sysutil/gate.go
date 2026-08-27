package sysutil

import (
	"context"
	"sync"
)

// Cross-pipeline resource governance.
//
// CPUBudget and everything derived from it — the Go parse-worker pool, LadybugDB's
// native thread pool, the ONNX intra-op pool — is a budget for ONE pipeline. Nothing
// stopped two pipelines from claiming it at the same time, and the daemon routinely
// runs several: one ProjectSupervisor per active project, each with its own sync and
// embedding module, all inside a single process. Three active projects therefore asked
// for three times the machine, plus a LadybugDB buffer pool per open database (two per
// database during a copy+swap rebuild) — which is how a 20-core host reached a load
// average of 30 with its swap exhausted while a supervisor held 10 GB resident.
//
// The gate is the missing half of that budget. These pipelines are batch work, so
// running them one at a time does not make the set finish later: it makes each one
// finish sooner, without thrashing, and it caps peak RSS at what a single pipeline
// needs instead of at N times that.

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
	// Checked before the select, and not only inside it. With a free slot and an
	// already-cancelled context BOTH cases are ready, and Go picks a ready case at
	// random — so roughly half the time a parked supervisor took a slot and started
	// a full reindex on its way out, which is the one thing the contract above says
	// it must not do. A free slot is the common case, which is what made this look
	// like a flaky test rather than the race it is.
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

// resetHeavyGate rebuilds the gate from the current environment. Tests only — it is
// not safe against a concurrent AcquireHeavy.
func resetHeavyGate() {
	heavyOnce = sync.Once{}
	heavyGate = nil
}
