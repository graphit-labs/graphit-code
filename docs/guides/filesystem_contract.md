---
title: Filesystem, State, and Watchers
type: guide
updated: 2026-10-30
tags: [filesystem, configuration, daemon, watchers, state, adapters]
references:
  - target: {type: task, id: tsk-8069}
    relation: implements
  - target: {type: task, id: tsk-749e}
    relation: documents
  - target: {type: task, id: tsk-a320}
    relation: documents
  - target: {type: task, id: tsk-1ed0}
    relation: implements
  - target: {type: task, id: tsk-1bf2}
    relation: implements
  - target: {type: task, id: tsk-9840}
    relation: implements
  - target: {type: task, id: tsk-8ee5, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-907c}
    relation: derived_from
  - target: {type: task, id: tsk-13ae}
    relation: derived_from
  - target: {type: task, id: tsk-c75a}
    relation: derived_from
  - target: {type: task, id: tsk-8bb7}
    relation: derived_from
  - target: {type: task, id: tsk-5035}
    relation: derived_from
  - target: {type: task, id: tsk-d3b4}
    relation: derived_from
  - target: {type: task, id: tsk-ffaa}
    relation: derived_from
  - target: {type: task, id: tsk-a8a3}
    relation: derived_from
  - target: {type: task, id: tsk-74f5}
    relation: derived_from
  - target: {type: task, id: tsk-0a82}
    relation: derived_from
  - target: {type: task, id: tsk-5d08}
    relation: derived_from
  - target: {type: task, id: tsk-6a07}
    relation: derived_from
  - target: {type: task, id: tsk-6b0a}
    relation: derived_from
  - target: {type: task, id: tsk-fa44}
    relation: derived_from
  - target: {type: task, id: tsk-d0f5}
    relation: derived_from
  - target: {type: task, id: tsk-cb52}
    relation: derived_from
  - target: {type: task, id: tsk-f49d}
    relation: derived_from
  - target: {type: task, id: tsk-c29b}
    relation: supports
  - target: {type: task, id: tsk-035c}
    relation: supports
  - target: {type: task, id: tsk-2006}
    relation: relates_to
  - target: {type: task, id: tsk-7bbb}
    relation: relates_to
  - target: {type: task, id: tsk-7bbb}
    relation: supports
  - target: {type: task, id: tsk-31f2}
    relation: relates_to
  - target: {type: task, id: tsk-31f2}
    relation: supports
  - target: {type: task, id: tsk-4b26}
    relation: supports
  - target: {type: task, id: tsk-2f54}
    relation: supports
  - target: {type: task, id: tsk-ced2}
    relation: relates_to
  - target: {type: task, id: tsk-1833}
    relation: relates_to
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
      type: task
      id: tsk-f4cd
    relation: supports
  - target:
      type: task
      id: tsk-78d4
    relation: supports
  - target:
      type: task
      id: tsk-d9b2
    relation: supports
  - target:
      type: task
      id: tsk-78d4
    relation: relates_to
  - target:
      type: task
      id: tsk-f4cd
    relation: relates_to
  - target:
      type: task
      id: tsk-cc0e
    relation: relates_to
  - target:
      type: task
      id: tsk-47a5
    relation: relates_to
  - target:
      type: task
      id: tsk-9b6d
    relation: relates_to
  - target:
      type: task
      id: tsk-d9b2
    relation: relates_to
  - target:
      type: task
      id: tsk-216a
    relation: relates_to
  - target:
      type: task
      id: tsk-4b26
    relation: relates_to
  - target:
      type: task
      id: tsk-3ddb
    relation: relates_to
  - target:
      type: task
      id: tsk-7a69
    relation: relates_to
  - target:
      type: task
      id: tsk-a388
    relation: supports
  - target:
      type: task
      id: tsk-0f61
    relation: relates_to
  - target:
      type: task
      id: tsk-00a4
    relation: supports
  - target:
      type: memory
      id: 01M49CSNFTV703GYZ7C0VTK0XA
    relation: derived_from
  - target:
      type: task
      id: tsk-650b
    relation: supports
  - target:
      type: task
      id: tsk-f30e
    relation: supports
  - target:
      type: task
      id: tsk-92b3
    relation: supports
  - target:
      type: task
      id: tsk-467f
    relation: supports
  - target:
      type: task
      id: tsk-3ba8
    relation: relates_to
  - target:
      type: task
      id: tsk-6a69
    relation: relates_to
  - target:
      type: memory
      id: 01M493837ABS01KQM54Y953ZRS
    relation: derived_from
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-b5d6
    relation: supports
  - target:
      type: task
      id: tsk-e079
    relation: supports
  - target:
      type: task
      id: tsk-1fee
    relation: supports
  - target:
      type: task
      id: tsk-8d62
    relation: supports
  - target:
      type: task
      id: tsk-6997
    relation: supports
  - target:
      type: task
      id: tsk-fcf0
    relation: supports
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
  - target: {type: task, id: tsk-250c}
    relation: derived_from
  - target: {type: task, id: tsk-3e75}
    relation: derived_from
  - target: {type: task, id: tsk-2706}
    relation: implements
  - target: {type: task, id: tsk-98e7}
    relation: supports
  - target: {type: task, id: tsk-e911}
    relation: derived_from
---

# Filesystem, state and watchers

This page tells users which files belong in a project and which are private generated state.
[Storage layout](../architecture/storage_layout.md) is the canonical physical layout; lower-level
filesystem contracts specify publication, ownership and recovery. All persisted/transmitted paths
are Unicode strings. There is no `os/units` codec or silent lossy path conversion.

## Project-owned files

| File or directory | Owner and effect | Share in Git? |
|---|---|---|
| `.graphit/project.json` | Stable ID, root metadata and `githooks`, ordinary `settings`; preserve across moves/worktrees | Yes, for the same project identity; secrets are rejected |
| `.graphit/rules/` | Maintained raw instruction overrides, under the template contract | Yes when authored for the project |
| `.graphit/ast/queries/` | Maintained extraction-profile overrides | Yes when intentionally customized |
| `.astignore`, `.wikiignore` | Source/document selection exclusions | Yes |
| `.agents/skills/` and host-native agent files | Effective installation, or user-authored content at that host's required destinations | Follow the team's policy; preserve user edits |
| Adjacent locks, temporary staging, runtime receipts | Generated synchronization/private publication state | No |

Identity is not inferred from a checkout pathname. Copying another project's identity is not a
safe way to create a different project. A missing/malformed identity is an explicit initialization
problem; do not delete it to fix a failed query. There is no productive project lockfile that
holds all module data. `graphit.lock.json` is not the current configuration or artifact registry.

## Global state

By default `<global>` is `~/.graphit`. `--global-dir` and the branded `GLOBAL_DIR` bootstrap
variable select another installation. Their value is not ordinary `config` state. Private roots
and credentials must retain their required OS access policy.

- `config.json`, `auth.json` and the global project inventory retain installation choices and
  identities; Auth is not a place for arbitrary ordinary settings.
- `projects/<project-id>/{ast,knowledge,memory,task}/` contains authoritative local corpora and
  derived graph publications. Versions use native Lance branches, not copied stores by checkout.
- `projects/<project-id>/artifact-store.json` and `hub.json` retain concrete installation origin,
  version, hash/ownership, pending publication and selected artifact state. A field is not a
  second content payload or a new authorization grant.
- `models/` holds canonical installed model artifacts. `tmp/` may contain private active model
  snapshots; a file named temporary is not by itself proven unowned.
- `daemon/`, private logs and platform service records belong to the installation, not a project.
- Verified USER Memory uses its own principal/installation root and is not a project branch.

Do not publish these files in Git or copy private credentials into hooks. Do not clear active
Lance fragments, branch refs, model snapshots or pending owned publications by filename guesses.
S3 providers access their authoritative remote tables directly; local caches are not a mirror to
edit or synchronize back later. See [S3](s3-and-ui-network.md).

## Agent adapter files

The selected native binding owns its destination and format. Kiro is V3, OpenCode is 2 and
Antigravity uses `agy`; there is no Kiro V2/preview coexistence or Gemini CLI compatibility layer.
Google Gemini inference protocols are independent from a retired CLI binding.

Use `graphit coordinator install`, `roles install`, `skills sync` and `hooks install` rather than
writing one generic format to every host. [Agent activation](agent_hook_activation.md) and the
[native binding reference](../development/native-host-bindings.md) specify exact destinations,
events and host permission semantics. Run tests with controlled protocol peers; actual usage of a
vendor model still needs that vendor's executable/account, but product tests do not require them.

The installed Graphit package is the canonical source of builtin templates. Installed files are
compared with the recorded content hash to detect user edits. There is no retained
`owned-payloads` directory of duplicate rendered content. Remote artifacts can retain the verified
canonical package/cache they actually need; removing that active cache is not harmless cleanup.

Generated hooks and MCP configurations use the executable on PATH and project-relative inputs.
Host launch environment supplies custom installation variables by name where supported, never
resolved private values in shared JSON/TOML. Static files contain normative instructions, not
runtime mandatory memories or private Role/Runtime credentials.

## Synchronize installations

```sh
graphit sync
graphit sync --agent codex
graphit skills sync --agent codex
graphit roles sync --agent codex
graphit coordinator sync --binding codex
graphit hooks sync --binding codex
graphit hub sync
graphit hub update --help
```

General sync reapplies installed exact Hub versions and updates native Graphit roles/skills,
Coordinator, hooks and MCP using current configuration. Component commands run the corresponding
smaller operation. An explicit Hub update selects newer remote releases. Sync is not a Git pull,
clone, automatic artifact upgrade, full index rebuild or wait for all embeddings.

Foreign files and content changed since installation produce conflicts according to the owning
publisher. Do not erase them to make sync pass. Inspect current content, choose the intended
source/customization and retry the supported operation. Independent successful steps can remain
committed when a later step fails; there is no cross-installation rollback promise.

## Git and agent lifecycle hooks

Product Git hooks bind PROJECT Memory/Task snapshots to commits and restore/merge their state.
`graphit git install/remove/snapshot/restore` controls them. This is separate from repository
quality gates and from agent lifecycle context hooks. Other Git/native-host entries remain owned
by their original authors. [Git records](../specs/git_records.md) covers conflicts and recovery.

Agent hooks load current module instructions/mandatory memory at their documented lifecycle
boundaries. Stop/SessionEnd requests an asynchronous PROJECT index rescan; it does not execute the
general publication sync or upgrade remote artifacts on every stop.

## What the daemon watches

The daemon retains registered checkout/version selections and captures relevant source,
documentation, project configuration and installation controls. Source changes wake per-project
incremental work; record changes can wake only semantic record work without walking AST/Knowledge.
Configuration changes are applied at their consumer's reload boundary. For example, model slots
are refreshed from global control changes; `ai.cli` captured by Ask requires daemon reattachment
(restart) to change its completion binding.

Ignored/generated directories do not become source merely because a watcher observed them.
Extraction settings, profile layers, custom image mappings and source fingerprints participate in
incremental reuse. Changing the producer must not leave old compiler facts eligible indefinitely.
[Ignore rules](ignore_files.md) and [indexing](indexing.md) describe actual selection and publication.

## Publication, safety and maintenance

Private filesystem writers use existing locks, captured identities/descriptors, validated relative
components, staging and atomic publication. These protect concurrent writers, confinement,
user modifications and interruption recovery; they are not artificial global resource quotas.
A pending intent/receipt can represent a real incomplete publication and must not be deleted to
fake success. Recovery must prove ownership before changing unknown files.

Native Lance maintenance compacts fragments and maintains indexes through SDK APIs, respecting
branch heads, retained Git tags and active snapshots. Icebug publication replaces changed FILE
shards and retires obsolete owned shards only when readers permit it. There is no `.lbug` store
to retain locally or periodic whole-corpus reconstruction for each edit.

For internal publication invariants use [Core/Hub storage contracts](../development/rust-storage-contracts.md),
[shared documents](../specs/native_shared_documents.md),
[artifact installation](../specs/hub_native_installations.md) and
[paths](../architecture/storage_layout.md). Never infer a cleanup guarantee from historical tests
on another platform or from the word `cache` alone.
