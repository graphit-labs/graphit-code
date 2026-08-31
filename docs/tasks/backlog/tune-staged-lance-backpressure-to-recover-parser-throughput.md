# Tune staged Lance backpressure to recover parser throughput

# Tune staged Lance backpressure to recover parser throughput

The generation-scoped staged search builder is correct and materially improves Linux reset wall time, but the bounded single-consumer design competes with parsing. On the 73,635-file Linux corpus, total fell from 423.0s to 327.5s and post-parse write fell from 270.58s to 86.01s, while parse rose from 151.92s to 240.96s. Active staged costs were prepare 186.82s, file/entity writes 3.76s/46.60s, and FTS 37.27s/45.37s. The queue is 16 entries, so backpressure quickly stalls result adoption and indirectly parser workers.

Measure queue depth/stall time and CPU/I/O contention. Compare larger but still bounded queues, separating row preparation from the single Lance appender, and explicit CPU budgeting between parse and preparation. Do not make the queue unbounded and do not weaken staging/rollback or vector-generation coherence. Also add age-guarded cleanup for abandoned `search.lance.staging.*`, `search.lance.backup.*`, and graph backup/discard directories left by process death.

Acceptance: on the Linux benchmark, preserve correctness and bounded memory while recovering a material portion of the ~89s parse regression; report parse, residual search wait, total wall, peak RSS, and cleanup behavior on Linux. Keep code and filesystem operations portable to macOS and Windows, with actual platform CI/build coverage where native dependencies permit.
