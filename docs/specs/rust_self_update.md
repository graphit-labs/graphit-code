---
title: Rust installed self-update
references:
  - target: {type: task, id: tsk-19ca}
    relation: implements
  - target: {type: task, id: tsk-20d9}
    relation: uses
---

# Rust installed self-update

The [App consumer](../../crates/graphit-app/src/self_update.rs) is registered and
the CLI now dispatches `graphit self-update` without requiring a PROJECT.
App24c qualified the ordinary guards and development CLI. App24f qualified the
real installed-daemon stop/restart and occupied-port failure journeys on Linux,
without installing a system service. App24g passed the affected normal-App
Clippy check. These results do not qualify other platforms or published releases.

## Captured inputs and replacement boundary

The invocation supplies its real launcher inputs, product Metadata version,
Original RequestContext and preparation limits. The consumer clones the existing
snapshot Arc; it does not reconstruct request identity or mint authority.
The CLI reads the compiled Metadata version and `std::env::current_exe`, then
uses the installation's global configuration and existing request policy. Copy
limits apply to the extracted package; the same configured byte ceiling bounds
each downloaded archive and its decoded TAR stream, including TAR headers and
padding. Metadata uses the existing content-input bound. These are per-operation
input limits. No corpus quota, resource reservation or new update channel is added.
The destination is the captured physical parent of current_exe. Its permissions
are preserved, including when that parent is public.

The [preparation producer](rust_self_update_preparation.md) selects and verifies
the maintained public release. Development and current-version outcomes return
before acquiring updater or daemon locks. A Ready owner retains its real private
staging envelope. Inner package directories preserve release modes; they need not
individually have mode 0700.

The replacement set includes graphit, graphit-model-server and graphit-runtime,
with platform-specific executable suffixes. CPU and CUDA builds use the same
layout; CUDA builds include their Cargo-built engine and NVIDIA runtime under `graphit-runtime/lib`, resolved through relative loader paths.

Configuration, credentials, profiles and trust are outside the replacement set.
Existing entries must have the expected regular-file/directory types. Staging
and backup share the target filesystem; no copy fallback or whole-package
atomicity is promised.

## Coordination and daemon lifecycle

A separate private .graphit-update control directory under the target parent
holds a permanent update.lock. Existing controls must already satisfy privacy;
the consumer does not repair their permissions. Core captures/revalidates parents
and FileLock serializes cooperating updaters. These are separate contracts:
FileLock does not provide parent identity, NOFOLLOW or hostile-mutation CAS.

The launcher observes authenticated status and PID. Stopping or status errors
abort. A missing status reply alone does not prove inactivity. The consumer
retains the same permanent daemon.pid FileLock that DaemonAuth::start uses,
without invoking startup or creating identity/runtime credentials.

A previously Running daemon is stopped through the real launcher before package
effects. The singleton remains held across replacement and recovery, then is
released before Restart and observed readiness. Restart does not install a
missing registration; Start must not substitute for it. An inactive installation
remains inactive. Singleton exclusion covers that installation/global directory;
it does not stop another installation sharing the same binary directory.

## Failures and retained recovery

Each existing package entry moves exclusively to an operation-owned backup
before its prepared replacement moves into place. An optional asset can be
added without a predecessor or retired without a replacement. A missing asset
declared by the prepared package remains an error. Conflicts never overwrite an
existing destination. A failure preserves its first cause and records whether
the package is unchanged, installed, restored or partial, along with observed
daemon effects.

Rollback moves only entries recorded by this operation while Original and parent
identity remain valid. It returns newly added assets to staging and restores
replaced or retired predecessors from backup. An expired or cancelled request does not acquire a new
cleanup authority. Necessary backups survive owner Drop when recovery cannot
finish. The failure reports their directory and separates rollback, cleanup and
restart errors from the first cause. A cancelled operation can therefore report
a stopped daemon or installed/partial package; it cannot claim no effects.

Successful replacement is distinct from successful restart and backup cleanup.
A restart failure retains the backup. A cleanup failure reports the remaining
operation-owned directory. TempDir::keep is the pinned library's persistence
operation; it prevents automatic deletion of recovery files
([tempfile 3.27 contract](https://docs.rs/tempfile/3.27.0/tempfile/struct.TempDir.html#method.keep)).
Recursive backup cleanup runs in `spawn_blocking`; its caller awaits that same
job before reporting completion. It does not promise immediate interruption of
an in-progress filesystem deletion.

The filesystem producer uses exclusive kernel renames on Linux/macOS and
MoveFileExW without replace/copy flags on Windows64. A running Windows executable
can still cause native replacement or cleanup errors. Linux fixtures do not
qualify that journey or installed Windows/macOS runtime behavior.

## Verification boundary

The [new tests](../../crates/graphit-app/src/self_update/tests.rs) consume a Ready
owner from the actual TCP/HTTP/checksum/TAR/filesystem preparation path. They
exercise package replacement/restoration, forward and inverse
rename conflicts, cancellation after an observed effect with retained backup,
and native target-lock contention with a permanent control entry. A further case
uses real launcher status and the same singleton lock to distinguish an inactive
installation from a busy lock despite absent discovery. The explicit native case
requires GRAPHIT_TEST_GRAPHIT_BIN and packages that real executable through the
same HTTP/checksum/TAR path. It checks authenticated stop/restart with a new PID,
then separately occupies the configured port to reject readiness while retaining
the installed-package effect and recoverable old package. The isolated HOME and
XDG runtime directory keep native service registrations outside the journey.

Code and Broker can share an installation prefix. Broker owns
`graphit-runtime/broker` and `graphit-runtime/licenses/broker`. Code self-update
preserves these independent namespaces with hardlinks while staging its runtime;
the displaced package stays complete for rollback. Unexpected links or a Code
release claiming either Broker namespace fail explicitly. Existing update locking,
cancellation and rollback rules continue to apply.
