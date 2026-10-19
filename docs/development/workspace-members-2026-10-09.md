---
references:
  - target: {type: task, id: tsk-8b93}
    relation: implements
  - target: {type: task, id: tsk-8bee}
    relation: derived_from
---

# Rust workspace integration — 2026-10-09

This configuration checkpoint lets maintainers validate five new Rust libraries.
It is part of the [Rust migration](rust-migration.md), not product cutover.

The workspace now has thirteen members. The five additions inherit edition 2024,
Rust 1.98, MIT licensing and workspace lints. Their manifests remain unchanged.

| Added member | Dependencies | Focused integration target |
| --- | --- | --- |
| `graphit-ast` | Standard library | `source_ranges` |
| `graphit-records` | `graphit-core` | `gates` |
| `graphit-ai` | `graphit-core` | `ranking` |
| `graphit-runtime` | `graphit-core`, `graphit-hub`, existing `serde`/`serde_json` | `output` |
| `graphit-knowledge` | `graphit-core` | `references` |

`cargo metadata --offline --format-version 1` resolved the additions in 0.344 s;
`cargo metadata --locked --offline --format-version 1` then passed in 0.155 s.
Both produced identical metadata. All 312 external package records retain their
name, version, source and checksum. Existing local package records are unchanged;
the lockfile adds only the five local packages and their declared dependency edges.
No workspace dependency or feature declaration changed.

Backups and raw metadata are in `.build/workspace-members-2026-10-09/`.
Its `receipt.json` records hashes, commands and target names; SHA256:
`ccbdcd2a5bb1f18bf0a31e89b9f349982da142b1f2c441af94120270344d4300`.
The previously accepted NativeProcessPrincipal evidence remains historical and
was not rewritten to use the new workspace hashes.

No build or product test ran in this configuration checkpoint. Each owner must
execute its affected integration, unit and documentation tests and strict Clippy
before accepting its library. Compilations use coordinated slots with two jobs
and incremental compilation disabled. These observations measure metadata
resolution only; they establish no product performance improvement.

At this configuration checkpoint, `cargo xtask check` performed safety,
formatting and workspace Clippy. The subsequent [fast runner correction](rust-fast-guard.md)
adds the short behavior tests; its real failure proof is tracked separately.
`cargo xtask gate` also runs workspace tests, including documentation tests;
it remains the broad integration barrier. The guard verifier copies the entire
`crates` directory, so these additions require no copying exception. No Go test
retirement, native Windows/macOS runtime validation, commit or push is implied.
