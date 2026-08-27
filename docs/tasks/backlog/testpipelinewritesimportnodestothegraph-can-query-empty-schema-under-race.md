# TestPipelineWritesImportNodesToTheGraph can query empty schema under -race

Flake: The test of importing can observe an empty graph/mid-rebuild

Observed on 2026-08-24 during `make test` of task `docs/tasks/configure-s3-and-ui-server-network.md`. The first execution failed in `internal/ast/cache_convert_imports_test.go:173`: `MATCH (i:Import)` returned a Binder exception because the table `Import` did not exist and the diagnosis indicated that there was no graph at all — either empty or mid-rebuild. No code from pipeline/import was altered on this task.

Observation of Reproduction: A complete round failed after 33.70 seconds. Immediately thereafter, `go test -race -tags lancedb ./internal/ast -run '^TestPipelineWritesImportNodesToTheGraph$' -count=3 -v` passed 3/3, and a second complete execution of `make test` passed.

Known Context: The memory INLINE_8__ documents that a correct query can receive a Binder exception while the swap/rebuild exposes a partial schema. This case was even stronger: no tables.

Investigate whether the test checks before the pipeline/swap is completely published, if there's unexpected sharing of DB/cache/home between parallel tests, or if the engine opens briefly the empty target during rename under race. Do not weaken the assertion that entity `Import` must exist.

Acceptance: Repeat the case alongside the suite AST on __INLINE_10__ without observing an empty schema; add synchronization/isolation to the producer or test as measured by the cause.
