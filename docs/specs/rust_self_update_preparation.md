---
title: Rust self-update release preparation
references:
  - target: {type: task, id: tsk-20d9}
    relation: implements
  - target: {type: task, id: tsk-19ca}
    relation: derived_from
---

# Rust self-update release preparation

The [Rust preparation module](../../crates/graphit-app/src/self_update_release.rs)
is registered in the App. The App21d ELF passed its ten initial focal tests for the earlier package layout.
It prepares an owned private package. The installed consumer and `graphit
self-update` are integrated separately in tsk-19ca; see the
[installed updater contract](rust_self_update.md) for replacement/restart proof
and its platform limits. Preparation itself grants no authority to install.

The caller supplies the actual captured product version, target, Original,
staging parent and finite limits. The product version comes from the existing
compiled Metadata producer, not Cargo's package version. A literal development
build returns Development without requesting a release; comparable versions use
the pinned SemVer library. An incompatible version fails rather than guessing an
ordering or switching a channel.

## Public release and integrity

Production requests the public GitHub latest release for graphit-labs/graphit-code.
It uses the real bounded HTTP producer, verified native platform trust, explicit
no-proxy policy and no personal credentials. Each finite redirect uses the actual
observed HTTPS origin and the same Original. A redirect body is rejected before
the first write callback; redirects, response bytes and headers remain bounded.

The [metadata helper](../../crates/graphit-app/src/self_update_release/metadata.rs)
selects exactly one archive and exactly one sidecar named by the maintained
[package producer](../../scripts/package-release.sh): graphit-PLATFORM.tar.gz
and graphit-PLATFORM.tar.gz.sha256. Repository, release tag and asset path must
match. Drafts, prereleases, missing/ambiguous assets and incompatible URLs fail.
Only the checksum line naming that exact archive is eligible; ambiguous lines,
size mismatch, truncation or a different streamed SHA256 cannot produce a
PreparedRelease. Signed URL queries never appear in its diagnostic representation.

Targets are the producer's linux-amd64, darwin-arm64 and windows-amd64. The
Windows executables carry the .exe suffix. A target fixture is layout evidence,
not evidence of executing native binaries on that operating system.

## Private stage and complete tree

The component creates its own temporary container and a directory born private
through the filesystem producer. The downloaded archive is born private through
the same producer. The completed owner retains that directory handle and the
temporary container; its explicit directory accessor grants no installation
authority. Errors discard only this operation's stage.

Extraction uses the pinned gzip and TAR libraries, in Tokio's existing blocking
pool. Reads retain the actual Original, cancellation and deadline. Captured
limits cover response and metadata bytes, decoded TAR bytes, file-content bytes,
actual nodes including implicit directories, total relative path bytes and depth.
No new runtime or detached installation task is created. If its waiter is dropped
during blocking work, the actual worker retains ownership until its bounded work
finishes. A dropped waiter is not a cleanup or join receipt.

Paths must remain in the exact package root. Duplicates, special entries,
absolute/escaping paths, writes through earlier links, outside/cyclic links and
unextracted entries are errors. Internal file symlinks are accepted only after
their real resolution is checked inside the validated tree. Existing regular
internal targets may also be hard-linked.

The result requires both executables, matching Cargo build provenance, runtime
notices and compiled-source license coverage, plus
agents/coordinator/AGENT.md and prompt.md. GGUF notices include engine and binding
licenses from the locked Cargo dependency, plus NVIDIA redistribution notices.
There is no custom SDK/backend receipt or separate accelerator module.

Linux packages carry the producer's `linux-libraries.json`. Preparation checks
its platform and the presence of declared libraries with normal relative names,
under the original cancellation/deadline. The complete archive receives its
streamed SHA-256 check before extraction; individual executables and libraries
are not hashed again. Native linking, ABI and runpath qualification remain with
the package producer. No host library probing or installed-system mutation
occurs in preparation.

CPU and CUDA builds share one package layout. The engine and optional CUDA
user-space runtime are packaged under `graphit-runtime/lib` with relative loader paths. Preparation validates the
package, not GPU availability or inference. No backend manifest is consumed. The
[native packaging contract](../development/native-runtime-packaging.md) remains
the source for the produced layout. These checks validate the staged package;
they do not attest a release signature, installed authority or native execution.

## Qualification

The [private unit fixtures](../../crates/graphit-app/src/self_update_release/tests.rs)
use actual TCP sockets and the real HTTP, filesystem, SHA256, gzip and TAR
producers. Their private loopback policy is passed to the same internal
composition; it is not a public endpoint option, injected client or fake grant.
Coverage includes three CPU target layouts, safe internal links, an optional
GGML accelerator layout, absent runtime libraries/notices and inventory path
escapes, plus exact metadata/checksum
selection, redirect-body suppression, truncation, invalid JSON/hash/archive,
missing Coordinator, duplicate/special/cyclic/escaping links, captured limits,
partial-body cancellation/deadline, future abandonment and private-stage removal.
The initial four metadata tests are separate from the six preparation tests.
App22c added a seventh preparation test: it holds the actual
Tokio blocking pool, drives real HTTP preparation until the archive is complete,
then drops the waiting future. Persistence of that stage while the pool remains
held proves ownership passed to the real worker; releasing the pool must allow
the operation's stage to disappear. Both normal work and Original cancellation
are covered without changing or replacing the production extractor.

The portable focal command is:

~~~text
cargo xtask cargo test -p graphit-app --lib self_update_release:: --locked --offline
~~~
