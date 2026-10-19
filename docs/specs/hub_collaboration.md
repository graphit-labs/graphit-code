---
title: Hub Collaboration Specification
content-type: reference
audience: developers
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-4208}
    relation: implements
  - target: {type: task, id: tsk-2006}
    relation: relates_to
  - target: {type: task, id: tsk-7bbb}
    relation: relates_to
  - target: {type: task, id: tsk-31f2}
    relation: relates_to
  - target: {type: task, id: tsk-4b26}
    relation: supports
  - target: {type: task, id: tsk-d83c}
    relation: supports
  - target: {type: task, id: tsk-2f54}
    relation: supports
  - target: {type: task, id: tsk-ced2}
    relation: supports
  - target: {type: task, id: tsk-a2bb}
    relation: supports
  - target: {type: task, id: tsk-84f3}
    relation: supports
  - target: {type: task, id: tsk-1833}
    relation: supports
  - target: {type: task, id: tsk-bc28}
    relation: supports
  - target: {type: task, id: tsk-aa20}
    relation: supports
  - target:
      type: task
      id: tsk-10b5
    relation: supports
  - target:
      type: task
      id: tsk-216a
    relation: supports
  - target:
      type: task
      id: tsk-9b6d
    relation: supports
  - target:
      type: task
      id: tsk-47a5
    relation: supports
  - target:
      type: task
      id: tsk-cc0e
    relation: supports
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target: {type: task, id: tsk-3ddb}
    relation: relates_to
  - target: {type: task, id: tsk-ad05}
    relation: relates_to
  - target: {type: task, id: tsk-e493}
    relation: relates_to
  - target: {type: task, id: tsk-7a13}
    relation: relates_to
  - target: {type: task, id: tsk-c3bf}
    relation: relates_to
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-47fd}
    relation: relates_to
  - target: {type: task, id: tsk-d75f}
    relation: relates_to
  - target: {type: task, id: tsk-b788}
    relation: relates_to
  - target: {type: task, id: tsk-4323}
    relation: relates_to
  - target: {type: task, id: tsk-376b}
    relation: relates_to
  - target: {type: task, id: tsk-427f}
    relation: supports
  - target: {type: task, id: tsk-81ba}
    relation: implements
---

# Hub collaboration and artifact lifecycle

The Hub shares versioned artifacts and native code/document contexts across projects and machines.
It separates authored producer sources, registry metadata, remote payloads, verified caches and
effective installations. Project discovery or knowing an artifact ID is not an access grant.
Use the [CLI guide](../guides/cli_reference.md) and [current MCP schemas](../reference/mcp-input-schemas.json)
for exact operations; [consumer contract](hub_consumer_contract.md) specifies authority/lifecycle.

## Identity and backend

A project has an immutable ID and a mutable discovery name. PROJECT version selects its records/
sources; artifact version selects a published artifact; these are not the same field. Native
context selectors retain exact artifact IDs/versions and aliases. A private brand can change
names/paths through supported branding, not by embedding one machine's checkout path.

Broker discovery/PROJECT rules and scoped S3 credentials authorize storage. Standalone providers
retain their own validated identity/topology/ACL contract. The client cannot choose an arbitrary
bucket/root or substitute local storage after a denied remote call. Object bodies move directly
between SDK/storage and Graphit. A cached registry or readable payload does not grant access.
See [Hub access](hub_access_control.md), [object layout](hub-s3-object-layout.md) and
[S3](../guides/s3-and-ui-network.md).

## Artifact families

| Family | Canonical source | Consumer |
|---|---|---|
| agent, skill, command, workflow, rule | Authored Resources or selected installed package | Host-native files rendered under the selected binding |
| MCP, language, power | Descriptor/content for that artifact kind | Existing host/Graphit-specific consumer, not automatic arbitrary executable authority |
| AST, Knowledge | Captured native SDK publication | Installed exact native context, local or authorized S3 on-the-fly |

A language artifact/profile alone cannot register another native recognizer. Custom parser Docker
routes are configured with `ast.scip.parsers`; native grammars are contributor build inputs.
Complete descriptors, validation, templates and required entries are in
[native installations](hub_native_installations.md). Do not invent a universal descriptor from
an old Go example when its current kind has a strict schema.

## Discover, review and publish authored sources

`hub authored list` enumerates maintained resource identities under the authorized producer
PROJECT/version. Review selects the exact kind/ID/artifact version and paginates entries; content
reads a selected relative text path. These readers capture sources through the PROJECT, not an
arbitrary user-supplied host root or copied internal codec in the App.

```sh
graphit hub authored list --project-dir . --limit 20
graphit hub authored review --help
graphit hub authored content --help
graphit hub publish --help
graphit hub publish-native --help
```

An absent authored namespace can list empty. Malformed metadata, unauthorized selection or changed
source fails explicitly. A read does not authorize publishing. FILE publication uses its captured
source and producer write grant; Native publication captures actual SDK snapshots. Version collisions,
source changes and uncertain effects remain producer outcomes, not silent overwrite or guessed success.

## Install and update

A remote install retains transport/provider/producer, exact artifact tuple, entry/alias, native
binding, scope, hash and concrete publication/ownership state. PROJECT and GlobalCache/GlobalNative
have distinct admissions. Another checkout of the same project can have its own effective native
files while logical corpus/version identity remains shared.

Copy is the initial physical default. Omitted update mode preserves an existing supported mode.
Rendered agent/skill/command/workflow outputs require Copy; Link is for eligible original sources,
not a retained duplicate render payload. Global cache operations reject a physical project mode.
A local source link's provenance is separate from its installation's physical mode.

`hub install-local` selects an explicit canonical local source with authorized producer and consumer
PROJECTs and literal artifact version. Reapplying captures current source and preserves omitted
alias/entry/mode choices. It uses the same writer/renderer and does not log in or fabricate remote
publication. [Local installation](../development/local-hub-installation.md) gives exact examples.

`hub update` explicitly selects newer declared remote releases. Without a single artifact selector,
it updates the recorded remote installations of the chosen target; local/Resources sources are not
silently turned into remote latest. Aliases/entries/physical modes stay selected. A missing declared
latest fails rather than guessing. Independent completed items remain committed if a later item
fails; results distinguish ok/error/not_attempted and actual observed effects. There is no atomic
rollback across all artifacts.

## Sync, user modifications and removal

`graphit hub sync` reapplies installed **exact versions**, sources and target choices. General
`graphit sync` also reconciles Graphit's roles/skills/Coordinator/hooks/MCP using their package
producers. It does not upgrade releases, clone repositories, select published Git ancestors or
wait for source/embedding completion.

The installed Graphit package is the canonical builtin instruction source. Destinations are the
effective installation; origin/version/hash detect edits. There is no `owned-payloads` duplicate
render store, payload pins or payload GC. Active remote packages/verified caches remain only where
actually needed for current readers/reapplication and are not the same as obsolete rendered copies.

The writer preserves foreign files and user modifications. A mismatch can be a conflict even when
another source happens to render similar bytes. Remove changes only within the established ownership
contract. `recover` resolves a real pending publication under current authority; `forget` drops
only the admitted installation metadata/effect defined by that operation. Neither authorizes
deleting unknown directories, adopting arbitrary files or claiming an uncertain write rolled back.
Unpublish removes the selected remote artifact version under producer authority, not a local install.

## On-the-fly Native contexts

AST and Knowledge packages capture domain snapshots and metadata. Import/install retains their exact
identity and readable native data rather than reconstructing it from a Git checkout. Local Icebug
and S3 Icebug both use in-memory Ladybug reading Parquet ranges; no persistent `.lbug` materialization
is required. Knowledge readers use the installed native corpus directly. Credentials and data
snapshots have independent lifetimes; current permission is checked again on consequential reads.

Use `ast imports`, exact context selection, typed Knowledge `source` and the installed artifact
catalog. Explicit local Source fallback requires an authorized retained project selection; an
installed graph is not a generic filesystem grant. [Native packages](../development/native-packages.md)
define export/import formats, integrity and provenance.

## Templates and native host customizations

All builtin instructions originate in maintained Markdown templates. Render with the actual Brand,
module tool names, skill aliases, artifact source and target binding. Raw PROJECT/Global overrides
and explicitly selected installed Hub bundles retain their documented precedence. A malformed
selected template fails instead of silently falling back. Do not embed private runtime mandatory
Memory, account secrets or checkout paths into static agent files.

`agents.<binding>.instructions` saves logical publication choices so later sync can reuse aliases,
bundles and module-tool settings. [Template contract](../architecture/rust-agent-instruction-templates.md)
and [instruction libraries](../architecture/rust-skill-instructions.md) specify inputs and escaping.
Host-native permissions are separate from Graphit's Role/Runtime authorization; tests use documented
controlled peers rather than requiring vendor plans or binaries.

## Process, recovery and implementation

Core owns filesystem capture/publication and small installation records. Hub owns artifact identity,
rendering, membership, pending operations and native binding semantics. App composes existing
producers for CLI/MCP/HTTP; it must not copy internal codecs or create another authority layer.
Native context data uses Data/Records SDKs. Broker owns ACL/discovery/STS, not a copy of every
client store or a proxy through which object bodies must flow.

Original cancellation/deadline, permissions, identity confinement, content integrity, concurrent
writer checks, joined processes and honest confirmed/uncertain outcomes remain required. Platform
support/publication claims need their corresponding executed artifact/consumer evidence; a router,
factory or controlled descriptor test alone is not a complete installed journey.
