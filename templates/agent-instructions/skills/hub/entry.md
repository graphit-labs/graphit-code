{% if tool_transport | default("root") == "role" -%}
# {{ brand }} Hub for this Role

Structured MCP calls accept top-level `ai_optimized`: omitted/`true` returns TOON text without duplicate JSON `structuredContent`; `false` selects JSON for machine consumers. Keep domain fields inside their advertised `input` envelope.

Use the installed catalog in the admitted PROJECT or installation. Read the exact artifact type, ID and literal version before choosing an artifact. A name or display description describes metadata; it does not grant access to its producer, storage or contents.

List the current PROJECT's installed metadata:
```json
{"input":{"scope":"project","limit":10}}
```
Call `{{ mcp_tool_hub_list }}`. The response contains `entries` and `next_offset`; follow the returned offset in the same query until it is null when the question needs the complete inventory. A missing result is different from a denied or failed read.

List installed graph libraries in the installation:
```json
{"input":{"scope":"global_installation","artifact_type":"ast","limit":5}}
```
Call `{{ mcp_tool_hub_list }}`. Keep producer, type, ID and exact version from the returned entry. Installation scope does not open other PROJECTs. Native graph or Knowledge contents require their respective admitted readers.

Inspect one concrete version with `{{ mcp_tool_hub_show }}`:
```json
{"input":{"scope":"global_installation","artifact_type":"ast","id":"vendor/graph","version":"release-two"}}
```
The example identity is illustrative; replace it with the real selection. Versions are literal: `latest` is not an inferred selector here. `artifact_not_installed` is a valid absence result. Do not substitute another version silently.

Remote FILE metadata is returned only while the current Role can access its configured origin and producer. Metadata inspection does not download or execute the artifact. Use only tools actually announced for this Role; a Runtime credential or a registry path cannot replace its grant.

Use `{{ mcp_tool_hub_content }}` for one text file in a selected installed artifact. Resources and approved local links are read through Core inside the admitted PROJECT; this local route needs Read(Hub), without RemoteRequest. It does not grant a sibling PROJECT or arbitrary disk path. For example:
```json
{"input":{"scope":"project","artifact_type":"mcp","id":"local/resource","version":"local-release","path":"config.json"}}
```

Remote FILE content additionally needs the current configured origin, producer authorization and an owned Core cache receipt. Standard tool-owned Scout/Tracker/Scribe ceilings remain unchanged; a custom Hub Agent needs an explicit permitted External(RemoteRequest) grant to use a remote service. For example:
```json
{"input":{"scope":"global_installation","artifact_type":"skill","id":"published/file","version":"literal-release","path":"SKILL.md"}}
```
Replace the illustrative identity with an observed installed FILE entry. `path` is a relative file inside that artifact; it cannot select arbitrary disk files. An absent or unverifiable cache is an error, not permission to download or substitute a local source. AST and Knowledge keep their separate admitted readers.


Discover remote PROJECT names with `{{ mcp_tool_hub_projects }}` only when this Role has its current remote Hub service authority. ReadHub alone does not authorize network operations. Select the configured transport/provider explicitly; a discovered PROJECT does not grant access to its artifacts:
```json
{"input":{"transport":"broker","provider":"broker","name_prefix":"prod","limit":5}}
```
Replace the illustrative configured provider/prefix with the real selection. `id` and `name_prefix` are mutually exclusive. Follow `next_cursor` with the same selection until the needed authorized inventory is covered.

For one returned PROJECT, call `{{ mcp_tool_hub_search }}` with `input.transport`, `input.provider`, its observed `input.producer` and `input.limit`. Optional `artifact_type`, `id` and `version` are literal filters. There is no free-text query, wildcard or inferred latest. A version filter selects matching manifests but returns all declared versions and the declared optional latest; `latest: null` remains null. Search reads manifests, not artifact bodies. Keep the returned cursor with the same query; restart from the first page when authorization or selection changes. Remote PROJECT authority and FILE content authorization remain separate from discovery.

Use `{{ mcp_tool_hub_type_path }}` with `input.artifact_type` and `input.name` for the current verified binding's destination advice. This is a lexical SDK projection, not an installed artifact, a filesystem read or permission to write. Keep the returned relative `path` and `placement`: a null path may mean dynamic context or no physical destination. For Agent, `agent_schema` identifies the supported schema; it is not a JSON schema body. Unsupported kinds fail explicitly. Do not pass a binding or private root to choose another host.

Read the scoped discovery companion through `{{ mcp_tool_module_skill }}`:
```json
{"input":{"module":"hub","reference":"references/discovery-cases.md"}}
```
{%- else -%}
# {{ brand_display }} Hub

`project_dir` is a runtime MCP argument, not persisted project identity. In docs, Task, Memory and handoffs save project identity plus repository-relative paths; never copy a machine-specific checkout root. Resolve the local root again on each host. Stores follow the checked-out project version (Git branch; `main` without Git); optional `project_version` reaches another.

{% include "shared/persistence-boundary.md" %}

Use this skill for named ecosystem projects, Hub artifacts and {{ brand_display }} configuration. Public libraries, frameworks and APIs do not require Hub discovery; use their official documentation when current details matter. Honor an explicit artifact request and reuse verified identities and versions.

## Resolve, select, read

Before unfamiliar project discovery, artifact selection or recovery, read [discovery through useful evidence](references/discovery-cases.md). Reuse that reference for related calls. If unavailable, call `{{ mcp_tool_module_skill }}` with `module: hub`, `reference: references/discovery-cases.md` and the known `project_dir` when applicable.

1. Resolve a named ecosystem project with `{{ mcp_tool_cluster_projects }}` in the initialized current project. Returned neighboring checkouts are {{ brand_display }}-managed: use their returned `dir` as `project_dir` for module tools, and read their relevant skill/overrides. Explore code through AST and contracts through Knowledge. Do not substitute native repository walks or invent local paths from a remote project ID.
2. If no local match exists, use `{{ mcp_tool_hub_projects }}` with the explicit configured transport/provider and pagination. Optional `id` or literal `name_prefix` selects projects; they are mutually exclusive. A returned `{id,name}` is discovery metadata, not PROJECT, storage or content authorization. Missing results and unavailable service are different outcomes.
3. Use `{{ mcp_tool_hub_search }}` for one explicit remote `producer` PROJECT. Optional `artifact_type`, `id` and `version` are literal filters; there is no free-text `query`, wildcard ID or separate metadata-show call. Inspect returned names/descriptions/tags within relevant pages. A version filter selects manifests containing that exact version while retaining their full versions list and declared optional latest; never infer a latest value. Follow `next_cursor` with the same selection. An authorization revision change requires a fresh first page.
4. Choose the actual installer for that kind. `{{ mcp_tool_hub_install_native }}` installs one exact AST or Knowledge NativeRoot into the global installation. `{{ mcp_tool_hub_install }}` installs a FILE artifact with explicit remote selection and either PROJECT/binding or global cache scope. Do not pass AST/Knowledge to the FILE installer or invent a PROJECT to stand for global installation. An obvious requested fit can be announced and installed; ask only when a material choice remains. Installation does not authorize execution of runnable content.
5. `{{ mcp_tool_hub_list }}` lists installed FILE metadata in its explicit PROJECT or global-cache scope; it is distinct from available remote search. Read installed FILE text with `{{ mcp_tool_hub_content }}` and its `installed` selection. A directory needs the selected relative `path`; omitting it does not request every file. Explicit `remote` content is a separate request-owned download. Neither route serves AST or Knowledge corpora. For Native contexts, read the module skill and its advertised installed-context/schema tools, then use the exact returned context identifier without inventing a checkout path.

Pass only fields advertised by each tool; do not add unadvertised `type`, `project_id` or legacy selectors to strict Hub payloads. Top-level `ai_optimized` is an optional strict boolean for structured MCP output: omitted/`true` selects TOON text without duplicate JSON `structuredContent`; `false` selects JSON for machine consumers. Follow pages while the decision needs coverage, without retaining entire corpora. A bounded miss, exhausted authorized inventory and service/authentication failure have different meanings. Report the covered scope and useful next action; a miss never proves global nonexistence.

## State changes

For an explicitly selected canonical local instruction directory, use the advertised Runtime tool `{{ mcp_tool_hub_install_local }}` with source_dir, producer, artifact_type (agent/skill/command/workflow/rule/mcp/language/power), full id, literal version and consumer project_dir/project_version. It requires current administrative PROJECT authority; a Role or Browser identity cannot substitute. This records LocalLink provenance, independently of optional physical mode copy/link. Reapply after edits; omitted alias/entry/mode preserve the current selection. Existing PROJECT lifecycle removes the projection while preserving the source. This is not authored discovery or a global install.

`{{ mcp_tool_hub_sync }}` reapplies the selected scope's installed Hub artifacts from their recorded origins and exact versions, including authored local sources. `{{ mcp_tool_sync }}` composes global/PROJECT Hub reconciliation with native Graphit skills, agents, Coordinator, hooks and MCP. Both require administrative Runtime authority, preserve user changes as explicit conflicts and report partial effects. Use them only when publication reconciliation is needed; they do not wait for background index completion.

`{{ mcp_tool_hub_update }}` updates one selected FILE installation, or the target's remote installations when selection is omitted; local authored entries remain unchanged. It can select newer remote releases, unlike sync. Preserve the actual producer, origin and destination selection. Publishing is a separate explicit action through the advertised FILE or Native publisher schema; discovery or installation does not authorize publication.

Remote Broker catalog retraction uses `{{ mcp_tool_hub_unpublish }}` with explicit provider, producer, artifact_type, full id and target. Select `target: {kind:"version", version:"literal-release"}` (optional current expected_sha256), or explicitly `target: {kind:"artifact"}` for all versions observed in the current catalog. It has no local PROJECT, binding or source selector. Current Runtime/Local HubWrite and Broker namespace authority are required; a discovery read or installed cache is not authority. Removing latest leaves it unset; no replacement is inferred. Payloads, local sources and installations are preserved. Keep the returned retraction and any late error outcome; inspect the original selection after uncertain or oversized delivery instead of blindly retrying. An empty error payload under a tiny output limit also requires inspection of that original selection.

PROJECT FILE lifecycle uses `{{ mcp_tool_hub_remove }}`, `{{ mcp_tool_hub_recover }}` and `{{ mcp_tool_hub_forget }}` with `scope: project`, full artifact ID, kind, PROJECT/version and binding. Optional artifact version is exact. These local operations require current administrative authority but no live remote provider or source cache. Recover and Forget accept only PROJECT. Remove additionally accepts explicit `scope: global_cache` or `scope: global_native` with a required exact artifact version, without PROJECT/binding fields. Global removal detaches registry metadata and preserves cached payloads and other versions; it is not physical deletion or recovery. GlobalNative requires its current Native installation authority, independently of remote HubRead/STS. Remove preserves foreign data; recover resumes only that key's real pending operation. Forget detaches metadata and same-key pending state while preserving physical leftovers and returning their preview. Preview paths do not grant deletion authority. Preserve partial effects, cleanup/pending and delivery failures; do not promise rollback or blindly replay a mutation.

Use currently advertised type/layout and configuration schemas when authoring or configuring artifacts; do not guess destinations. `{{ mcp_tool_config_get }}`/`{{ mcp_tool_config_list }}` inspect keys, and `{{ mcp_tool_config_set }}`/`{{ mcp_tool_config_unset }}` change them within the authorized work. `{{ mcp_tool_cluster_get }}`/`{{ mcp_tool_cluster_set }}`/`{{ mcp_tool_cluster_unset }}` handle project grouping.

Record resolved project, producer, artifact identity, concrete version and consequential decisions in Task. Knowledge keeps contracts; Memory keeps reusable operating decisions. Reference evidence instead of copying artifact bodies or saving host-specific roots.
{%- endif %}
