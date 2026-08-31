# Split sync --heavy timing into embedding generation and IVF-PQ finalization

# Split sync --heavy timing into embedding generation and IVF-PQ finalization

The search-index latency work in `docs/tasks/explain-search-index-and-streaming-graph-export.md` moved IVF-PQ from synchronous `ast index` to `sync --heavy`. The normal index path now reports its subphases, but `runSyncHeavyTasks` still reports a single task duration that combines missing-embedding inference, Lance vector upserts, IVF-PQ training, fold, and maintenance. A real run with 1,518 invalidated embeddings took 178.3s, while a second run with zero missing embeddings isolated vector finalization at 27.8s; operators currently need two runs to distinguish those costs.

Add explicit heavy timing fields or progress events around: pending scan, provider/model initialization, inference plus vector writes, IVF-PQ creation, fold, maintenance, and status publication. Keep the ready-generation fast path visible as a no-retrain outcome. Avoid changing the pending/ready generation protocol or moving FTS/source-table work out of synchronous AST publication.

Acceptance: one `sync --heavy` run prints non-overlapping durations for embedding work and vector-index finalization, and tests assert the zero-pending path reports IVF-PQ work without initializing the embedding provider.
