# TestPipelineWritesImportNodesToTheGraph can query empty schema under -race

# Flake: the Import test can observe an empty/mid-rebuild graph

Observed on 2026-08-24 during `make test` for the task `docs/tasks/configure-s3-and-ui-server-network.md`. The first run failed at `internal/ast/cache_convert_imports_test.go:173`: `MATCH (i:Import)` returned a Binder exception because the `Import` table did not exist, and the diagnostic reported that the graph had no table at all — empty or mid-rebuild. No pipeline/import code was changed in that task.

Reproduction observed: one full run with `-race -tags lancedb` failed after 33.70 s. Right afterwards, `go test -race -tags lancedb ./internal/ast -run '^TestPipelineWritesImportNodesToTheGraph$' -count=3 -v` passed 3/3, and a second full `make test` run passed.

Known context: the memory `LadybugDB liga propriedade se ALGUMA tabela candidata tem — e schema parcial durante rebuild finge ser query errada` documents that a correct query can receive a Binder exception while the swap/rebuild exposes a partial schema. This case was even stronger: zero tables.

Investigate whether the test queries before the pipeline/swap is fully published, whether there is unexpected sharing of DB/cache/home between parallel tests, or whether the engine briefly opens the empty target during the rename under race. Do not weaken the assertion that the `Import` entity must exist.

Acceptance: run the case repeatedly alongside the AST suite under `-race -tags lancedb` without observing an empty schema; add synchronization/isolation in the producer or in the test according to the measured cause.
