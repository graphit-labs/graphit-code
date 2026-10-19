---
title: Rust remote artifact version selection
type: reference
status: implemented
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-5372}
    relation: implements
  - target: {type: task, id: tsk-97a7}
    relation: derived_from
  - target: {type: task, id: tsk-cc0e}
    relation: derived_from
  - target: {type: task, id: tsk-e493}
    relation: derived_from
  - target: {type: task, id: tsk-4208}
    relation: supports
  - target: {type: task, id: tsk-1dd5}
    relation: supports
---

# Rust remote artifact version selection

The pure `graphit_hub::remote_version` API selects an existing member of a
validated [remote artifact manifest](remote_artifact_metadata.md). It returns
`&RemoteVersion` borrowed from the same manifest, preserving its producer,
artifact type, complete published ID, version spelling, digest and input order.
The result is catalog data. Authorization, payload integrity, fetches, hydration
and source leases belong to the separate publisher and resolver integrations.

## Selectors

Call `select_version(manifest, selector, limits, original)` with explicit
`VersionSelectionLimits` and the original `policy::RequestContext`:

| Selector | Meaning | Example |
|---|---|---|
| `Latest` | Exact member named by the manifest's optional `latest` | Published latest `1.2.0` stays `1.2.0` even if `1.3.0` exists |
| `Exact(&ConcreteVersion)` | Literal identity comparison | `feature/hub-✓`, `v2.0.0`, prerelease and build spellings stay exact |
| `ReleasePrefix { major, minor }` | Highest stable full SemVer release in that numeric family | Major 1, minor 2 selects `1.2.9` over `1.2.0` |

Application transport normalization is separate. Omitted or literal `latest`
maps to `Latest`; a dependency's literal empty selector also maps to `Latest`.
Explicit user empty/null input remains invalid at the transport boundary. Numeric
families `1` and `1.2` mean major/minor families, without caret compatibility.
Full numeric and named identities use `Exact` and preserve their spelling.

Publish full three-component SemVer identities for numeric family selection.
Stored `1` and `1.2` remain valid exact identities but cannot serve as full
SemVer release candidates. Prefix selection parses with
[semver 1.0.28](https://docs.rs/semver/1.0.28/semver/struct.Version.html), removing
at most one lowercase `v` only for classification. Prereleases and non-SemVer
names are excluded from that scan; exact lookup continues to support them.

Release comparison uses `Version::cmp_precedence`, disregarding build metadata.
Different identities tied at the winning precedence, such as `1.2.3` and
`v1.2.3`, or `1.2.3+one` and `1.2.3+two`, return `AmbiguousVersion`. A tie among
lower releases does not hide a later higher unique release. Input order never
breaks a winning tie. Missing `latest` returns `LatestUnavailable`; it never
infers a numeric or lexical maximum. An absent exact identity or release family
returns `VersionNotFound` without initiating a download.

## Current limits and interruption

`max_versions` and `max_version_bytes` must both be positive. Selection validates
the borrowed exact selector, optional latest and every stored version under
these current capacities, even if decoding used a broader policy. An oversized
unrelated entry rejects an exact/latest request. An oversized absent exact
selector returns `Limit` before `VersionNotFound`. No successful lookup bypasses
the complete preflight.

The original context is checked at entry, in batches of at most 32 versions in
preflight and selection, and before returning a result. Permanent cancellation
and the existing absolute deadline produce `Cancelled` and `Deadline`. The
library never constructs a replacement context, cancellation or clock, resets
the request deadline, or performs filesystem/network IO.

Work is linear in manifest entries and bounded version input bytes. The scan
retains a borrowed winner and at most two parsed SemVer candidates; it does not
clone or sort the version list. Parser allocation is limited by accepted input
length, without a hard RSS or global allocator guarantee. Closed static errors
are `InvalidPolicy`, `Limit`, `Cancelled`, `Deadline`, `LatestUnavailable`,
`VersionNotFound`, and `AmbiguousVersion`. Display messages expose no raw
selectors, document, provider URL or credentials.

## Implementation and qualification

Implementation lives in `crates/graphit-hub/src/remote_version.rs`; fixtures are
in `remote_version/tests.rs`. The public module and direct `semver = '=1.0.28'`
dependency are integrated. The existing fast catalogue selects `remote_version::`.

The fixtures decode validated manifests for all ten artifact types, compare
borrowed member pointers, preserve order/digests, and exercise explicit latest,
literal names, numeric precedence, winning ties and current input limits.
Public requests reject permanent cancellation and elapsed deadlines. A direct
test of the same private gate used at entry, each scan batch and final delivery
checks an admitted original request before and after cancellation or real expiry.
No test hooks, thread-local scheduler, subprocess harness or replacement clock
are needed. This test covers cooperative gates, without claiming cancellation
was injected during a public scan.

Affected qualification commands:

```text
cargo test --locked -p graphit-hub --lib remote_version::
cargo test --locked -p graphit-hub --lib remote_metadata::
cargo clippy --locked -p graphit-hub --lib --tests --no-deps -- -D warnings
```

The simplified Linux selection suite passed six tests, with no ignored cases.
The affected metadata suite passed nine tests; its existing bounded serialization
child is executed by its parent. Hub library/test Clippy, formatting and whitespace
checks passed. Existing project hooks continue to select the selection tests.
These checks qualify this pure change without another full fast-suite campaign
or configured-hook injection.

Pure selection does not complete `tsk-4208` publication or `tsk-1dd5` resolution.
It establishes no endpoint, credential/grant producer or ecosystem delivery.
