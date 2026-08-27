Nothing coordinates the buffer pool between processes: two writers at 8 GB on a machine with insufficient memory cause both to fail.

Nothing coordinates the buffer pool between processes.

Observed on August 19, 2026 during the correction of the buffer pool ceiling (commit c2deb27, task log `docs/tasks/fts-build-starved-by-a-flat-buffer-pool-ceiling.md`). Not corrected: larger scope than the reported bug and the paper ceiling has already removed the risk to the long-running process.

## O defeito

The inline_1 (internal/ast/resources.go) resolves the ceiling **by handle** within a process, starting from the total RAM of the machine. With a write ceiling set to 8 GiB, N concurrent writers request N × 8 GiB and nobody sums this total against what the machine actually has available.

Inside/inside (internal/sysutil/gate.go) serializes the heavy pipelines **within the daemon** — it is a process-level serialization. Does not cover:

---

This translation maintains the technical context and structure of the original Portuguese text, preserving code blocks, markdown formatting, file paths, and technical terms as specified.

- one inline 5 of terminal competing with the indexer reindexing the same or another project
- __INLINE_6__, which opens many read-write databases in a single process
- two simultaneous user CLI invocations

Evidence measured

In this machine (total size 61 GB, but ~11 GB available and the swap space is full for other loads):

- The `go test ./internal/ast/ -run 'Search|Ladybug|BoundedDB|Rebuild|Incremental'` is running alongside real indexing → **`signal: segmentation fault (core dumped)`** in 37.6 seconds
- The same selection, alone, with ~22 GiB available → **OK in 94.9 seconds**
- The `TestFTSBuildUnderBoundedBufferPool` with 2.5 million entities and a 6 GiB pool on an underpowered machine → SIGSEGV within the engine at ___INLINE_10__; the same `TestFTSBuildUnderBoundedBufferPool` with 8 GiB constructs the nine indexes in 129.4 seconds

In other words: exhaustion doesn't always come back as an engine error. Sometimes it comes back as a Cgo crash, which you can't catch inside Go.

What has already been discarded

- "It's not the 8 GB limit that's wrong." It is the minimum measured for 2.5 million entities (~3 GB per million). Downloading it fixes the original bug.
- "Not `CHECKPOINT`."
Measured: with CHECKPOINT between each `CREATE_FTS_INDEX`, the failure is identical and each checkpoint returns in 0.00 seconds.
- "It's not the read limit." Handles for read-only have been set to 1 GB purposefully, exactly to avoid inflating the daemon and MCP.

Possible Outputs (choose one, do not stack)

1. **Gate between processes**: a lockfile in the directory of the brand with the same semantics as `AcquireHeavy`, acquired by any process that opens a read-write handle indexation file. Solves the real case (CLI × daemon) and is the closest to the design already existing.
2. **Estimate against available memory DISK**: `sysutil` only exposes `MemoryLimitBytes()` (total or cgroup). A `MemoryAvailableBytes()` reading `MemAvailable` from `/proc/meminfo` would allow cutting off the ceiling when the machine is tight. Simpler, but the number changes between calculation and usage — mitigates without resolving.
3. **Estimate requirements before starting and fail early**: The count of entities is known before the FTS build (`entities.rows` in `RebuildFromCache`). With ~3 GiB per million measured, it allows comparing with handle pool and failing with actionable message `ftsBuildError` ANTI earlier than wasting minutes and possibly crashing. Does not prevent containment, but replaces a core dump with an understandable error.

The 1 and the 3 are complementary and together they hold value; the 2 is the weakest.

How do you know it worked?

Run `go test ./internal/ast/` concurrently with `graphit ast index` in a large corpus on an intentionally tight machine and obtain two legible results (one waiting for the other or an actionable error) instead of SIGSEGV.
- No regression in `TestBoundedDBBufferPool` nor in the probe `GRAPHIT_FTS_BUFPOOL=1` with 2.5 M / 8 GiB.
