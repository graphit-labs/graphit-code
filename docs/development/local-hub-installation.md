---
title: Install canonical local Hub instructions
status: focused-linux-qualified
references:
  - target: {type: task, id: tsk-9316, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-0257, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: supports
---

# Install canonical local Hub instructions

The Native CLI and Runtime MCP installer accepts a canonical local directory for a FILE artifact (agent, skill, command, workflow, rule, MCP, language or power) and publishes its instructions into a retained PROJECT. This consumer reuses the maintained renderers and the owned Core/Hub publication transaction. Windows and macOS native execution remain unrun.

Use an absolute source directory. The installer captures its canonical root; the
input need not already use the canonical spelling. A final directory symlink is
rejected. Skills contain SKILL.md; Agents
contain their maintained AGENT.md descriptor; Rules contain RULE.md. Command and Workflow entry selection
uses the existing binding planner, including an optional entry_name. Companion
files are retained for directory skills. Markdown variables such as {{brand}} and
{{mcp_tool_ast_source}} are resolved by the captured instruction renderer before
native binding rendering. Invalid content fails rather than falling back to a
raw copy. A provider executable, login or paid account is not required to render
these files.

```sh
graphit --project-version main hub install-local --source-dir /work/instructions/review --producer 01J00000000000000000000001 --kind skill --id team/review --version release-one --project-dir /work/consumer --binding codex --alias review
```

The producer is explicit provenance, independent of the consumer PROJECT. The
artifact version is literal; latest is rejected. The PROJECT version selects an
existing retained version. Omit it only when that root is unambiguous. The current
captured binding is used when binding is omitted. The source path, producer, alias
and a valid login grant no PROJECT or administrative permission.

The corresponding Runtime MCP operation is graphit_hub_install_local with a strict
object:

```json
{"source_dir":"/work/instructions/review","producer":"01J00000000000000000000001","artifact_type":"skill","id":"team/review","version":"release-one","project_dir":"/work/consumer","project_version":"main","binding":"codex","alias":"review"}
```

There is no provider, profile, transport, global selector or remote manifest in
this request. An explicit null physical mode and unknown fields are rejected.
This is a Runtime management operation; Role and Browser authority cannot be
substituted. It requires the existing Runtime administrative permissions Configure,
ManageCache and PublishedFilesystem, the current installation root and an active
PROJECT identity/version among its captured allowed destinations. Authentication
and PROJECT selection precede source filesystem reads. Original cancellation and
deadline, current policy, identity, live membership and publication scope are
rechecked at the existing effect boundaries.

SourceRef::LocalLink records the canonical original source. It is separate from
ArtifactMode::Link, which selects physical projection. Rendered publications use
Copy and reject link mode before publishing: the installed destination is the
rendered output, and no extra payload copy is retained as a symlink target.
The canonical source remains unchanged. Source file links are rejected by the
existing copy policy.

Edit the canonical source and repeat the same command to recapture and render it.
There is no implicit source watcher. Reapplication preserves the current alias,
entry selection and physical mode when omitted; an explicit value requests a
change through the same writer. Reusing a target owned by another origin,
producer or canonical source is a conflict. Destination foreign files are preserved.

To remove the owned projection, use the existing exact PROJECT lifecycle:

```sh
graphit --project-version main hub remove --kind skill --id team/review --version release-one --project-dir /work/consumer --binding codex
```

Removal preserves the canonical source. Recover and Forget use the same target,
PROJECT/version, kind and binding. Recover resumes only the selected key; Forget
preserves physical leftovers and reports their preview. Inspect committed,
pending, possible_parents and cleanup facts after an error before deciding the
next action. A lost response does not establish rollback and is not a reason to
replay a mutation blindly.

Success returns status ok, operation install_local, source local, the selected
producer/artifact and PROJECT identities, binding and the actual publication
change. Failures use the existing finite error envelope with the known outcome,
including publication not_attempted before dispatch and actual committed/pending
facts after an effect. Local malformed content and excessive payloads are
invalid_local_artifact and local_artifact_too_large; authorization, bad arguments,
missing artifacts, conflicts and Original expiry retain the current Hub codes.

This cut does not provide authored-source discovery, remote unpublication,
Browser installation, global destinations or imports of old registries. The
[Hub caller contract](../specs/hub_consumer_contract.md) tracks those consumers
separately. Linux execution and any other platform qualification must be recorded
explicitly; source-level availability does not prove native platform execution.

## Web installation from an authored PROJECT source

The Web LocalLink consumer selects a maintained Resources identity instead of a host directory.

The route is `POST /api/hub/authored/install`. Select the source from
[authored Resources discovery](../specs/hub_consumer_contract.md#authored-resources-discovery-and-review)
and supply the authorized destination PROJECT separately. Both require an explicit
PROJECT ID and retained data version. The source additionally selects an exact
artifact type, full ID and literal artifact version. Only the existing Native
local types `agent`, `skill`, `command` and `workflow` are accepted.

```json
{
  "source": {
    "project_id": "01J00000000000000000000001",
    "project_version": "main",
    "artifact_type": "skill",
    "id": "team/review",
    "version": "release-one"
  },
  "scope": {
    "kind": "project",
    "project_id": "01J00000000000000000000002",
    "project_version": "main"
  },
  "binding": "codex",
  "alias": "review"
}
```

The IDs above are illustrative; use selections returned by the authorized live
PROJECT consumers. `binding` is required. Optional `alias`, `entry_name` and
`mode` follows the existing physical installer. These rendered instructions
require `"copy"`; `"link"` is rejected before publication. Omitted mode uses
Copy on first installation and preserves the current mode on reapplication;
omitted alias and entry name preserve their current values on reapplication.
Explicit null, unknown/duplicate fields, unsupported kinds, `latest` and a Global
consumer fail validation. The body contains no host path, producer override,
provider, storage credential or Runtime grant. The producer comes from the
selected source PROJECT's actual identity.

Authorization is independent for each side. The verified Web caller must have
PROJECT Hub/read on the source. The destination uses the existing
`admit_web_hub_project_mutation` authority: explicit private Web administration
policy, current PROJECT Hub/read and the actual destination physical capture.
There is no Broker PROJECT Hub/write permission to request. Source read alone does
not grant installation, and destination administration grants no source read.
No storage service or STS is needed for this local installation. Cookie, Origin,
request headers, Original, current project membership and owned job cleanup remain
the existing Web mutation ingress controls.

The selected Resources tree is captured by Core and retained/rechecked during
installation. `SourceRef::LocalLink` records that canonical source, its real
producer and literal artifact version. The maintained renderer and Core/Hub writer
apply Copy/Link, alias and entry intent; a rendered physical Link points at the
owned rendered payload. Reapplication recaptures source edits. Changing an
existing target's origin, producer or canonical source is a conflict. Removal and
recovery use the destination's existing exact lifecycle; removal preserves the
original source and can proceed when that source is unavailable.

Success uses the existing mutable FILE body: `status:"ok"`,
`operation:"install_local"`, destination `scope`, `cache:"not_attempted"`,
`source:"local"`, source `producer`, artifact identity, destination
`project/project_version`, `binding`, `publication` and `possible_parents`.
The publication change is the existing string, for example
`"Physical(Created)"` or `"VerifiedUnchanged"`. Refresh installed inventory for
actual alias/entry/mode and `LocalLink` provenance; these are not invented echoes.

A business failure returns the existing `error.code` and any observed `outcome`;
validation or admission can fail before an outcome exists. A publication failure
can report `publication:{committed,hub_pending,core_pending,observation_failed}`
with boolean values, or retain an already-confirmed publication string.
`cleanup_pending:true` appears only when observed. There is no generic `pending`
field; absence is not a false value. Worker observation failure can report
`{change:"Unknown",committed:null}`. Output overflow retains the existing bounded
inspection response instead of truncating an identity or receipt. Inspect effects
before retrying or choosing Recover/Forget; an HTTP error does not prove rollback.

The implementation reuses the mutable Web job/envelope and the existing file installation lifecycle.

## Reconciliation

`graphit hub sync` and Runtime `graphit_hub_sync` reapply registered local sources and remote FILE installations at their exact recorded versions, preserving aliases, entry selection and modes. They do not upgrade releases; use `hub update` for that. General `graphit sync` / `graphit_sync` includes this producer and native Graphit skills, roles, Coordinator, hooks and MCP. A conflict or missing source is explicit and preserves previous effects. Native local AST/Knowledge packages are verified through SDK admission without reimport, reindexing or changes to Lance history.
