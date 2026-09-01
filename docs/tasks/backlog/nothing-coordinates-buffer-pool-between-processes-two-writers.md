# Nothing coordinates the buffer pool BETWEEN processes: two writers at 8 GiB on a machine with no available memory bring both down

# Nothing coordinates the buffer pool between processes

Observed on 2026-08-19 during the fix of the buffer pool ceiling (commit c2deb27, task log `docs/tasks/fts-build-starved-by-a-flat-buffer-pool-ceiling.md`). Not fixed: a scope larger than the reported bug, and the per-role ceiling already removed the risk for the long-running process.

## The defect

`boundedDBBufferPool` (internal/ast/resources.go) resolves the ceiling **per handle, inside one process**, starting from the machine's TOTAL RAM. With the write ceiling at 8 GiB, N concurrent writers ask for N × 8 GiB and nobody adds that total up against what the machine actually has available.

`sysutil.AcquireHeavy` / `HeavySlots` (internal/sysutil/gate.go) serializes the heavy pipelines **inside the daemon** — it is a process-level `chan struct{}`. It does not cover:

- a `graphit ast index` from a terminal competing with the daemon reindexing the same or another project
- `go test ./internal/ast/`, which opens many read-write databases in a single process
- two user CLI invocations at the same time

## Measured evidence

On this machine (61 GiB total, but ~11 GiB AVAILABLE and swap full from other loads):

- `go test ./internal/ast/ -run 'Search|Ladybug|BoundedDB|Rebuild|Incremental'` running alongside a real indexing → **`signal: segmentation fault (core dumped)`** in 37.6 s
- the SAME selection, on its own, with ~22 GiB available → **ok in 94.9 s**
- the `TestFTSBuildUnderBoundedBufferPool` probe with 2.5 M entities and a 6 GiB pool, on the tight machine → SIGSEGV inside the engine at `se_tri`; the same probe with 8 GiB on a machine with room builds the nine indexes in 129.4 s

That is: exhaustion does not always come back as an engine error. Sometimes it comes back as a cgo crash, which cannot be caught from inside Go.

## What has already been ruled out

- **It is not the 8 GiB ceiling being wrong.** It is the measured minimum for 2.5 M entities (~3 GiB per million). Lowering it brings the original bug back.
- **It is not `CHECKPOINT`.** Measured: with a CHECKPOINT between each `CREATE_FTS_INDEX` the failure is identical and each checkpoint returns in 0.00 s.
- **It is not the read ceiling.** Read-only handles were left at 1 GiB on purpose, precisely so as not to inflate the daemon and the MCP.

## Possible ways out (pick one, do not stack them)

1. **A gate between processes**: a lockfile in the brand's directory with the same semantics as `AcquireHeavy`, acquired by any process that is about to open a read-write indexing handle. It solves the real case (CLI × daemon) and is the closest thing to the design that already exists.
2. **Budget against AVAILABLE memory, not total**: `sysutil` only exposes `MemoryLimitBytes()` (total or cgroup). A `MemoryAvailableBytes()` reading `MemAvailable` from `/proc/meminfo` would allow cutting the ceiling when the machine is tight. Simpler, but the number changes between the calculation and the use — it mitigates without solving.
3. **Estimate the requirement before starting and fail early**: the entity count is known before the FTS build (`entities.rows` in `RebuildFromCache`). With the measured ~3 GiB per million, it is possible to compare against the handle's pool and fail with `ftsBuildError`'s actionable message BEFORE spending minutes and possibly crashing. It does not prevent the contention, but it trades a core dump for a legible error.

1 and 3 are complementary and are worth having together; 2 is the weakest.

## How to know it worked

- Run `go test ./internal/ast/` concurrently with `graphit ast index` on a large corpus, on a deliberately tight machine, and get two legible outcomes (one waiting for the other, or an actionable error) instead of SIGSEGV.
- No regression in `TestBoundedDBBufferPool` nor in the `GRAPHIT_FTS_BUFPOOL=1` probe with 2.5 M / 8 GiB.
