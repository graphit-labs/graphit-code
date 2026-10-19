{% if tool_transport | default("root") == "role" -%}
# Role documentation design and consistency

Use this companion for a scoped review or an authorized authoring handoff. Retrieval alone needs no documentation campaign. Only this run's captured PROJECT/version and granted tools are available; another project's name, context or authored reference does not grant access.

## Establish the reader's goal

Locate the authoritative DOCS_ROOT and current navigation. Retrieve relevant maintained contracts, user conditions and Task intent. A focused `{{ mcp_tool_knowledge_search }}` uses the Role input:

```json
{"input":{"query":"deployment policy","top_k":5,"page_size":2,"preview":true}}
```

Read a selected slug with `{{ mcp_tool_wiki_source }}`; replace the illustrative slug with a returned one:

```json
{"input":{"slug":"alpha","selectors":{"head":20,"line_numbers":true}}}
```

Map actual actors, journeys, observable rules, boundaries, evidence and gaps. A source tree is implementation layout, not the reader's domain model. For a narrow correction inspect only that domain and real dependents. A whole-system request needs its actual domains, not a generic page standing in for them.

## Choose useful pages

Follow the existing navigation. Add a domain overview, user guide, technical contract or operational page only when it serves an independently useful goal. Use audience, scope/version, prerequisites, steps, outcomes and errors a new reader can act on. Define terms before using them. File counts and headings do not prove coverage. Link shared rules instead of repeating an entire specification.

Do not invent retention, compatibility, migration, access or recovery behavior. Separate a current contract from a proposal or unresolved gap. Use concrete source/symbol references and logical project identities; host roots do not belong in maintained content. A Source excerpt, a passing test and an observation each prove only their stated scope.

## Verify both directions

For a code change, follow its observable behavior to affected pages, examples and navigation. For a documentation change, inspect the actual implementation and relevant tests. Classify drift as stale documentation, an authorized code defect, proposed behavior or an unresolved conflict. Do not weaken the requirement to bless a bug or create a feature solely to make a draft true.

Check changed examples through the actual advertised schemas and handlers. Preserve missing, denied, incomplete and uncertain outcomes. Record what was executed versus inspected, which source/page targets agree and what remains unresolved. A read-only Role returns these findings to its coordinator; it does not claim acceptance, checkpoint or completion authority.

## Worked domain

For an unfamiliar domain set, request [the worked case](worked-domain.md) through `{{ mcp_tool_module_skill }}`:

```json
{"input":{"module":"knowledge","reference":"references/worked-domain.md"}}
```

Adapt the depth and page placement to the real domain. The example is not evidence that its fictional product rules exist in this PROJECT. Rich reference resolution, KnowledgeLog and Hub publication require their own actual tools and admissions; do not substitute them with file reads or invented commands.
{%- else -%}
# Designing and maintaining domain documentation

Read before a documentation set or coverage review. Use the per-unit consistency section for focused maintenance. This is a decision process, not a mandatory directory scaffold.

## 1. Discover the domain before proposing pages

Locate the configured DOCS_ROOT and current navigation, page conventions, language, supported releases and authoritative owners. Read relevant Knowledge pages, retained Task intent/decisions and selected AST entrypoints, contracts and tests. Use bounded searches around the requested capability; inspect cross-domain callers when they change the contract. A source tree describes implementation layout, not automatically the reader's domain model.

Identify:
- Audiences and goals: end users, administrators, integrators, operators, maintainers; include only real audiences.
- Journeys: trigger, prerequisites, normal sequence, completion signal, alternate paths, errors and recovery.
- Business rules: actor/permission, conditions, invariants, limits, lifecycle and deliberate exclusions. Preserve exact user constraints.
- Boundaries: owning component, consumers, external dependencies, authoritative data, supported interface/version and operations.
- Evidence: page/source/test/decision supporting each claim; distinguish observed behavior, intended behavior, assumption and unresolved gap.

For a whole-system request, inventory its business domains and their interactions before drafting any overview. For a narrow change, map only its affected domain plus real dependents. Unknown facts do not justify invented defaults for retention, access, error semantics or compatibility. Resolve consequential gaps through sources or focused clarification; keep unrelated work moving.

## 2. Build a coverage map and choose placement

Save a compact map in the active Task or a maintained domain overview when that is useful to readers:

| Domain / actor / journey | Rule or contract | Current evidence | Page / section | Gap / next action |
|---|---|---|---|---|
| A real capability and who needs it | Observable condition and outcome | Exact source path/symbol, existing page or requirement | Existing target or justified new location | Covered, stale, missing, partial, conflicts or explicitly deferred |

Use this to decide the documentation work units and their checks in Task. A row is covered only when a reader can act on its page and its claims have support. A filename, headings, keyword match or a passing generic lint check is not coverage. Map all relevant journey classes: primary, alternate, exception, recovery and operational constraints; explain deliberate exclusions rather than creating empty sections.

Choose the smallest coherent set:
- Root/index: audience entry points and domain navigation, with scope/version and links to shared concepts.
- Domain overview: vocabulary, business purpose, boundaries, rules, journeys, dependencies and navigation.
- User guide: one reader goal or coherent journey, with prerequisites, steps, outcomes and recovery.
- Technical reference/design: stable contracts, architecture, data/state, integration boundaries, trade-offs and provenance.
- Operational guide: deployment/migration/recovery or diagnosis when an operator has a real distinct job.

Adapt to the existing project. A CLI can need command/input/output/exit-status contracts; a library needs call/return/error/lifetime contracts; a compiler needs syntax, diagnostics and compatibility; an API needs request/response/access/error contracts. Do not force HTTP, a web UI or a fixed language onto a different project.

Split by audience, independently useful journey, contractual boundary or maintenance ownership. Merge closely related material whose separation would force repetitive context. Do not impose a file minimum or leave one broad architecture page standing in for missing domain guides. Preserve working links/slugs when moving content; update navigation and inbound references. Link shared rules to one authoritative definition, with enough local explanation to make each procedure usable.

## 3. Write pages a new reader can use

Start each page with audience, goal, scope and applicable version/status. Use the project's metadata conventions; do not invent unsupported wiki frontmatter. Define domain terms before relying on them. Prefer concrete examples and observable outcomes over adjectives such as simple, secure or robust.

**User guide:** identify required role/access, starting state, environment/data and effect of the operation. Give ordered actions using real labels or commands; show meaningful input and expected output/state. Include how to verify completion and what remains unchanged when it matters to the decision. Cover actual failure/empty/retry cases and a recovery or escalation path. Keep internal file paths out of the main user journey; link technical detail.

**Technical page:** explain component responsibilities and request/data flow, then specify the public contract with examples. Include input defaults/validation, outputs, errors, permissions and integration effects. For mutable data, describe identity, relationships, constraints, state transitions, atomicity/concurrency/retry semantics and lifecycle. Document compatibility/version gates, configuration, migration and operational signals where relevant. Name exact verified paths/symbols and tests so another agent can find the owner. Record consequential decisions and why plausible alternatives were rejected; separate a design decision from a claim about measured behavior.

**Operational page:** state when to use it, required access, safe diagnostic steps, expected signals, interpretation, recovery and verification. Derive commands from the actual interface. A migration/rollback recipe must say what state changes, compatibility conditions, what can fail and when to stop. Do not invent monitoring names, retention periods or performance promises.

Write complete examples with internally consistent IDs, roles, states, versions and results. Mark hypothetical examples clearly. Retain facts that affect a reader's decision, not the conversation transcript. Avoid copying entire source files, test suites, the same architecture explanation or the full project spec into every page.

## 4. Per-unit consistency: code → docs and docs → code

This applies to EVERY code or documentation work unit, not only a final documentation task.

Before changing code, identify affected behavior, rules, consumers and authoritative user/technical pages. After the change, follow the changed entrypoint/contract to those pages, examples, navigation and dependent domain references. Update them within the authorized work. A rename can affect a command example or source reference even if runtime semantics stay the same.

Before changing documentation, compare each changed factual claim with corresponding implementation, tests and supported versions. After writing, read the affected implementation again only if it changed or the claim needs unresolved evidence; do not repeat identical tool calls merely to satisfy a ritual. For a spelling-only edit, checking the unchanged referenced behavior and links can establish no behavioral impact; do not fabricate runtime tests.

Classify discrepancies and resolve explicitly:
- **Stale docs:** implementation and authorized intent agree; update pages/examples and affected references.
- **Code defect:** behavior violates an established requirement; fix within authorized scope and validate, or preserve the truthful current limitation and register remediation in Task. Do not rewrite the requirement to bless the bug.
- **Future design:** intended behavior has not shipped; keep it labeled proposed/planned with its requirements and implementation gap. Do not present its command/API as currently usable.
- **Conflicting intent/evidence:** name both sources and the unresolved decision; resolve consequential ambiguity before claiming the affected behavior is verified.
- **No impact:** record the inspected docs/code targets, relevant contract and why no content change is needed. No-impact is a supported conclusion, not an automatic checkbox.

Review two separate kinds of quality:
1. **Documentation quality:** coverage, specificity, terminology, navigation, source traceability, internal consistency and usability without this conversation. Are prerequisites, defaults and failure/recovery paths actually explained?
2. **Implementation agreement:** targeted source inspection, existing tests, contract checks or safe execution of examples demonstrate the stated behavior for the named version and conditions.

Passing the first does not imply passing the second. Record actual commands/observations and limitations in Task checks, including source/page targets. Do not claim executed tests when only inspected. Check both directions across the affected coverage rows before completing the unit; unresolved discrepancies remain findings with a next action, not a false “consistent” result.

## 5. Make knowledge usable from another project

An external reader cannot see the author's checkout, shell state, unpublished plan or conversation. Supply product/domain scope, supported versions, prerequisites, terms, public contracts, data meanings, realistic use/error examples and operational limits. Use stable page links and repository-relative source references with revision/release provenance; machine-specific checkout roots do not belong in maintained docs, specifications or evidence, whether local or published. project_dir belongs only in the live MCP envelope; resolve it per host. For example, cite project Workboard, src/projects/list.go::ListProjects and its revision, not an expanded checkout path.

For consumption, retain the runtime PROJECT selection or the exact imported artifact ID/version through Native search and source reads. Use the Hub skill for unfamiliar project/artifact resolution; public technologies do not require a Hub detour. For publication requested within scope, review the target, included material and version through Hub, then publish with the supported tools. Maintaining docs prepares reusable knowledge but does not itself authorize publication. Distinguish artifact/source version from actual deployed behavior.

Record reusable decisions or discovered system constraints in Memory, while Task keeps the evidence, coverage work and remaining actions. Neither replaces the maintained pages.
{%- endif %}
