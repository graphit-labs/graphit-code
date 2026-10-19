{% if tool_transport | default("root") == "role" -%}
# Scoped installed discovery for this Role

Start with the PROJECT already admitted to this Role. For a local installed artifact, retain its complete returned identity: artifact type, ID, producer, literal version and installation target. Titles and aliases help the reader choose; they do not replace identity or authorize a new source.

To page an installed graph-library inventory, call `{{ mcp_tool_hub_list }}`:
```json
{"input":{"scope":"global_installation","artifact_type":"ast","limit":1}}
```
When `next_offset` is present, pass that offset with the same filters to continue. Inspect a chosen version through `{{ mcp_tool_hub_show }}`:
```json
{"input":{"scope":"global_installation","artifact_type":"ast","id":"vendor/graph","version":"release-two"}}
```
Replace the illustrative identity with an actual entry. An absent entry is not proof that the producer or a remote artifact does not exist. A denied or unavailable origin must remain a causal failure; do not retry with a different transport, provider or identity merely to bypass it.

The PROJECT and installation catalogs describe different scopes. A foreign agent binding is not selected by this Role's local binding. A remote FILE entry requires current service and producer authorization even when the metadata is already cached. Native graph metadata does not hydrate the graph; use the admitted AST or Knowledge tools for its contents.

For destination advice, call `{{ mcp_tool_hub_type_path }}` with `input.artifact_type: "skill"` and `input.name: "producer/review"`, replacing the illustrative name with the intended literal identity. The tool uses the Role binding already verified for this call. A relative path is a projection for that binding, not an observed directory or evidence that an artifact is installed. Rules may use dynamic context; other kinds may have no physical destination. An Agent result supplies the exact supported `agent_schema` identifier rather than an invented schema document.

Installed runnable artifacts are not authorized for execution by a discovery read. This connection exposes only its announced tools: do not copy Root command-line payloads, add `project_dir` or put `ai_optimized` inside Role `input`, or turn private storage paths into artifact IDs. Continue with a different source only when that source has its own real admission.
{%- else -%}
# Discovery through useful evidence

Read this before unfamiliar project discovery, artifact selection or recovery. These names, IDs, provider and paths are fictional examples of supplied responses, not identities to invent in real work. Use actual returned selections and configured providers. Save logical identities and repository-relative references; resolve host roots anew.

## Local project first

For “explain XPTO's approval contract,” call `{{ mcp_tool_cluster_projects }}` in the initialized current project. If it returns `/work/xpto`, keep that exact target for its Knowledge, AST and other module calls. A neighboring checkout is still {{ brand_display }}-managed. Read its relevant module skill via `{{ mcp_tool_module_skill }}`, query the needed contract and inspect implementation through AST when asked. Do not turn the neighboring project ID into an imported artifact or fall back to native code discovery merely because it is outside the current directory.

Keep the coordinating Task in its owning project. Record which contract/page/symbol supports each conclusion, what remains uncertain and the exact next action. Reading another project does not authorize editing it. Reuse prior Task or Memory evidence when a new question needs its rationale; do not mechanically query every store.

## Published project and manifest selection

If the current cluster has no match, discover visible published projects with `{{ mcp_tool_hub_projects }}`:

~~~json
{"transport":"broker","provider":"hub-service","name_prefix":"Experience","limit":20}
~~~

Assume the returned page identifies “Experience Portal” with ID `01J00000000000000000000001`. Follow `next_cursor` with the same provider/filter when needed. Names are clues; the project ID remains the producer selection. Discovery itself grants neither artifact access nor storage credentials.

List its available Knowledge manifests with `{{ mcp_tool_hub_search }}`:

~~~json
{"transport":"broker","provider":"hub-service","producer":"01J00000000000000000000001","artifact_type":"knowledge","limit":20}
~~~

Inspect the returned ID, name, description, tags, declared latest and versions. For a known selected ID and exact version, the same tool accepts:

~~~json
{"transport":"broker","provider":"hub-service","producer":"01J00000000000000000000001","artifact_type":"knowledge","id":"xpto/contracts","version":"2.3.0","limit":20}
~~~

The `version` filter tests existence and still returns all declared versions. No free-text query, inferred latest, separate show call or globally unique title is involved. If the user requested a React skill, select an actual visible producer and inspect its `artifact_type: skill` pages; use a known literal ID when available. Do not send the phrase as an unsupported query or infer that a bounded miss means no such skill exists anywhere.

After selecting the requested contract, announce the useful preparation. Install this exact Knowledge NativeRoot with `{{ mcp_tool_hub_install_native }}`:

~~~json
{"installation":{"provider":"hub-service","producer":"01J00000000000000000000001","artifact_type":"knowledge","id":"xpto/contracts","version":"2.3.0"}}
~~~

The installation is global; no fictional PROJECT or FILE cache request is needed. Success registers that exact release, but does not prove its contents. Read the Knowledge skill, resolve its actual installed context with the advertised module tools and retrieve the relevant pages. For implementation evidence, separately discover/install the appropriate AST artifact and version. Do not reuse a Knowledge ID as an AST context or invent an on-disk checkout from either result. Each reader still needs its own current authorization.

## Remote catalog retraction

After an explicit request to withdraw an observed published release, call `{{ mcp_tool_hub_unpublish }}` with its actual configured Broker provider and producer:

~~~json
{"provider":"hub-service","producer":"01J00000000000000000000001","artifact_type":"skill","id":"xpto/review","target":{"kind":"version","version":"release/one"}}
~~~

This is a global remote mutation, with no project_dir, project_version, transport, binding or source path. A current Runtime/Local HubWrite lease and the producer namespace grant authorize it. Optional expected_sha256 applies only to the selected version. To withdraw the whole artifact catalog, replace target with `{"kind":"artifact"}` explicitly; omission never means all versions. A version removal preserves other releases and clears latest only when it points at the removed release. Last-version and whole-artifact retraction remove the catalog conditionally. Payloads, project metadata, local source and installed copies remain intact.

Success returns status unpublished and retraction with the observed targeted_versions, desired_latest and catalog updated or removed. A failure may retain rejected, uncertain or a known committed effect, including inside outcome after cancellation or deadline. Oversized results require inspection of the same remote selection; an empty protocol error under a tiny output limit cannot prove no effect. Inspect before retrying; do not infer rollback or substitute an installed catalog.

## A FILE artifact in a consumer PROJECT

Assume the selected skill is `xpto/review`, producer `01J00000000000000000000001`, exact version `release/one`. Install it into the known consumer `/work/portal` with `{{ mcp_tool_hub_install }}`:

~~~json
{"selection":{"transport":"broker","provider":"hub-service","producer":"01J00000000000000000000001","artifact_type":"skill","id":"xpto/review","version":"release/one","exact":true},"global":false,"project_dir":"/work/portal","project_version":"main","binding":"codex"}
~~~

The consumer PROJECT is distinct from the remote producer. Only use a configured supported binding. An optional alias changes the destination name, not the artifact's identity. A global FILE cache install instead uses `global: true` without PROJECT, binding or alias fields; it is not an AST/Knowledge NativeRoot install.

After confirmed installation, read its entrypoint with `{{ mcp_tool_hub_content }}`:

~~~json
{"installed":{"artifact_type":"skill","id":"xpto/review","version":"release/one","global":false,"project_dir":"/work/portal","project_version":"main","binding":"codex","path":"SKILL.md"}}
~~~

Read further files only as needed. Directory content needs a relative path; omission does not download all files. Current origin authorization is still checked for content. Installation or discovery alone does not authorize execution of the skill's commands or publishing a release.

## Local removal and pending work

An unavailable remote provider or removed source cache does not prevent authorized local PROJECT FILE cleanup. Use `{{ mcp_tool_hub_remove }}` for the exact installed target:

~~~json
{"artifact_type":"skill","scope":"project","id":"xpto/review","version":"release/one","project_dir":"/work/portal","project_version":"main","binding":"codex"}
~~~

To resume that target's existing pending operation, call `{{ mcp_tool_hub_recover }}`:

~~~json
{"artifact_type":"skill","scope":"project","id":"xpto/review","version":"release/one","project_dir":"/work/portal","project_version":"main","binding":"codex"}
~~~

Foreign files may prevent recovery. If the authorized purpose is to detach metadata while retaining physical leftovers, use `{{ mcp_tool_hub_forget }}`:

~~~json
{"artifact_type":"skill","scope":"project","id":"xpto/review","version":"release/one","project_dir":"/work/portal","project_version":"main","binding":"codex"}
~~~

Inspect the returned preview, known effects and pending/cleanup facts. Forget is not a successful physical removal and does not grant permission to delete the reported paths. Another key's pending operation is refused instead of being recovered as a side effect. Remove a concrete installed global-cache metadata version with `{{ mcp_tool_hub_remove }}`:

~~~json
{"artifact_type":"skill","scope":"global_cache","id":"xpto/review","version":"release/one"}
~~~

For a Native library, select its distinct global target explicitly:

~~~json
{"artifact_type":"knowledge","scope":"global_native","id":"xpto/contracts","version":"2.3.0"}
~~~

Both global selections require an exact version and reject PROJECT, binding,
alias and remote selection fields. Removal detaches that version's local registry
metadata, preserving payloads and all other versions; it does not delete cached
bytes or create physical pending work. GlobalNative still requires current Native
installation authority; metadata or a readable JWT does not grant it. No HubRead,
STS or remote payload access is requested merely to remove local metadata.
Recover and Forget accept PROJECT only: global metadata cannot enter the physical
publication journal. Use actual advertised surfaces for other scopes, without
translating old command names into guessed aliases.

## Misses, conflicts and handoff

| Observation | Useful next action |
| --- | --- |
| No match in a bounded project or manifest page | Follow relevant pages or another actual producer/filter while the request needs coverage; report the inspected scope. |
| Authorized inventory is exhausted | State that no match was found in that inventory; do not infer global absence. |
| Service is unconfigured, denied or unavailable | Report the causal failure; catalog emptiness is unknown. Inspect configuration when relevant and authorized, without resetting credentials. |
| Cursor asks for restart after a selection or authorization change | Start a fresh first page under the current selection; a cursor is a position, not permission. |
| Multiple publishers have similar names | Preserve producer plus kind/full ID/version and explain a material choice before installing. |
| Requested version is missing | State the gap and actual alternatives; never silently substitute another release. |
| Mutation fails after an effect or delivery is ambiguous | Preserve the result's actual effects/pending and establish current state before retrying. Do not claim rollback or context availability. |
| Published documentation differs from local code | Keep each project/context/version and resolve the relevant contract through Task, Knowledge and AST. |

A useful handoff identifies the question, local logical project or remote producer/artifact/version, why it was selected, which actual pages/symbols were read, conclusions, limitations and next action. Shared records contain no credentials, private storage addresses or machine-specific roots. Stop retrieval once the decision has adequate evidence.
{%- endif %}
