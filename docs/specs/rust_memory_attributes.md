---
references:
  - target: {type: task, id: tsk-b129}
    relation: implements
  - target: {type: task, id: tsk-613a}
    relation: derived_from
  - target: {type: task, id: tsk-09f5}
    relation: supports
  - target: {type: task, id: tsk-7154}
    relation: implements
  - target: {type: task, id: tsk-c558}
    relation: integrates
---

# Rust Memory attribute aggregation

Audience: Records maintainers and Memory consolidation consumers. The main API and automatic-fold callsite are implemented under `tsk-7154`. Eight main kernel tests and eight affected native automatic-operation tests passed on Linux; scoped strict checks, formatting and documentation verification passed. Historical `tsk-b129` and isolated `tsk-c558` receipts do not qualify this integration.

The [Memory records contract](rust_memory_records.md) defines consolidation, preservation, authority and publication. The helper combines scalar attributes of members validated by its consumer; the supplied Core `SnapshotContext` remains its runtime dependency.

## API and aggregation

Source: `crates/graphit-records/src/memory_attributes.rs`, exported by `crates/graphit-records/src/lib.rs`.

```rust
merge_attributes(
    sources: &[MemoryAttributes],
    context: &SnapshotContext,
) -> Result<MemoryAttributes, MemoryAttributeError>
```

`MemoryAttributes` contains classification, mandatory and important. Classification precedence is `Correction > Convention > Decision > Tension > Skill > Fact`. Both flags are ORed independently across every source. A mandatory Fact, an important Skill and an unflagged Correction therefore produce a Correction with both flags set. These fields are metadata, never grants.

Input is borrowed and immutable. Order and repeated identical sources do not change the scalar result. The iterative fold takes O(N) time and O(1) additional memory, without successful-fold allocation, recursion or I/O. Core may allocate when constructing a request error.

The helper checks the same context at entry, before each source and before success. `EmptyInput` rejects an empty slice after the entry check. `Request(io::ErrorKind)` retains cancellation/deadline failure; entry request failure precedes empty input. The caller preserves the original request and its admitted lineage.

## Actual consumer

`memory_store/automatic.rs` combines observed attributes with this two-argument API. Its physical input limits, member IDs, expected revisions, complete observations, current heads, Auth, live fences and conditional publication remain separate controls. Attributes do not prove member identity, content completeness, survivor preservation or permission to mutate records.

Consumers preserve evidence, provenance and references and publish the survivor with final archives/tombstones through the [transaction boundary](rust_record_transactions.md). Automatic application `tsk-74e7` and references `tsk-29fb` retain their separate historical receipts; the callsite cleanup has new affected qualification under `tsk-7154`.

## Focused qualification

Eight kernel fixtures preserve independent flags/input immutability, all classification pairs, 720 permutations, duplicate idempotence, empty input, actual cancellation/deadline, iterative ten-thousand-source input and shared-original cancellation after success. The eight affected native automatic-operation tests exercise observed aggregation, archive/publication, history/reobservation, isolation, refusal and integrity/cancellation.

```text
cargo test --locked --offline -p graphit-records --lib memory_attributes:: -j1 -- --test-threads=2
cargo test --locked --offline -p graphit-records --test memory_store automatic:: -j1 -- --test-threads=2
cargo clippy --locked --offline -p graphit-records --lib --test memory_store --no-deps -j1 -- -D warnings
rustfmt --edition 2024 --config skip_children=true --check crates/graphit-records/src/memory_attributes.rs crates/graphit-records/src/memory_store/automatic.rs
```

Current evidence belongs in `.build/rust-memory-attribute-main-cleanup/qualification.json`. This qualification is Linux-only; it does not qualify live Runtime/Dream, Windows/macOS or full product cutover.
