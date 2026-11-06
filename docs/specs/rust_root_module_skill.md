---
title: Rust Root module skill
type: reference
status: qualified
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-0257}
    relation: implemented_by
  - target: {type: session, id: ses-c98a}
    relation: derived_from
---

# Rust Root module skill

Audience: Root MCP integrators and Runtime maintainers. In graphit-code,
`tsk-0257` implements `graphit_module_skill` and its focused tests. The
coordinator has integrated Source catalogue/dispatch and proxy selection.
The integrated reader, Brand registration and catalogue fixture correction
have compiled successfully. Seven distinct focused Root cases are functionally
qualified: five preserved passes and two exact passes in Root51d. Affected
Global/App/Root strict now passes in the explicit 51z package/target scope;
written assertions or formatting alone do not establish that PASS.

The reader returns maintained core-module guidance through the real
[skill templates and instruction renderer](../architecture/rust-skill-instructions.md).
It does not retrieve live Memory/Task/USER context, issue Role grants, invent
Hub artifacts or publish skills. [PROJECT hooks](rust_project_hook_context.md)
retain their separate live-context contract.

## Flat request and scope

There is no Role `selection`/`input` envelope. The schema is read-only and
rejects additional properties.

| Field | Contract |
|---|---|
| `module` | Required string; trimmed and ASCII-lowercased to `ast`, `hub`, `task`, `memory` or `knowledge`. The advertised enum uses canonical spellings. |
| `reference` | Optional exact catalogue relative path. Empty or omitted selects the body; an unknown path fails instead of reading an arbitrary file. |
| `project_dir` | Optional Unicode path string. When supplied, its absolute path must select a real retained PROJECT. |
| `ai_optimized` | Optional strict boolean: omitted/`true` returns a TOON envelope; `false` returns JSON. |
| `project_version` | Optional nonempty version; valid only with `project_dir`, through the existing roster/version selector. |

Explicit null optionals, wrong types and unknown fields fail. Captured
input/scalar limits and NUL validation apply. For example, the neutral Global
body request is `{"module":"ast"}`; its exact companion request is
`{"module":"ast","reference":"references/impact-assessment.md"}`.

Omitted `project_dir` means neutral Global, including with an empty PROJECT
roster. The Root proxy does not inject cwd/project/version defaults for this
tool. Global Knowledge rendering injects neither a daemon PROJECT nor a
PROJECT/installation docs root. Explicit PROJECT uses real live selection,
version and physical capture; `HookPolicy::selected_configuration` reads its
current configuration under the same Original. PROJECT Knowledge obtains its
validated root from `compiled_skills::docs_dir`, without an invented cwd or
`docs/` default. Relevant enablement/docs-root changes before delivery fail.

## Authority, templates and output

Root HTTP requires the actual native `runtime` or `local` origin and bearer,
authenticated through the respective existing caller producer. Every scope
requires `Read(selected_module)` with `Obligation::Unconditional`. Global
retains physical installation and caller rechecks. PROJECT additionally uses
`ProjectRecordAuthority::admit_authenticated_read` and final recheck under
the same caller/Original, captured installation/PROJECT, version and binding.
A Role label, observed metadata or template is not admission.

The configured installation root is resolved to the canonical root held by Auth.
Global configuration, templates and Broker instructions use that same resolved
destination throughout the call; an input alias cannot redirect later reads.
Canonical path equality does not replace physical installation revalidation.

`ModuleSettings` supplies the response's `enabled` observation. An authorized
disabled module still returns its guidance with `enabled: false`; enablement
never grants read permission. No Runtime bearer is renamed Local and no
RoleGrant or Hub proof is synthesized.

The active contract is the Rust instruction template bundle:
`rules/skills/<module>/entry.md` with renderable companions and captured
precedence. `skill_templates`, `capture_template` and `render_instruction`
select and render it. Markdown overrides are captured from the explicit
PROJECT's `<brand-directory>/rules/skills/...` and the installation's
`rules/skills/...`, with Project > Global > compiled priority under the engine.
Companion/shared candidates are captured and validated together. Selected
invalid body/reference/companion input fails without lower-origin fallback.
Captured sources, including observed absence, are revalidated before delivery.
Exact reference names select the framework catalogue; their contents follow
the selected captured template bundle rather than a forced compiled body.
The reader installs or repairs nothing and does not assert availability or
grants for operations named in text. This is the intentional Rust instruction contract.

Success is a structured envelope, represented as TOON text by default (no duplicate
JSON `structuredContent`) or JSON with `ai_optimized: false`. The envelope is
`{module, name, enabled, content}`: canonical module, Brand skill name,
effective boolean and complete rendered Markdown. A reference response adds
`reference`; a body response instead adds sorted `references` when the
catalogue is nonempty. Body and reference are separate documents. The rendered Markdown remains the
original `content` string; ModuleSkill is not a raw-text exception. Guidance and metadata are not runtime snapshots or authority.
JSON-RPC, arguments and catalogue schemas remain JSON.

Response skill names and template tool/skill values derive from the captured
installation Brand. Source now builds `definition(&Brand) -> io::Result<Tool>`
at startup, retains that Tool in State and uses its `Tool.name` for get/list/call
and dispatch, aligned with the Root proxy's Brand-derived name. The
Orbit-Brand proxy Global/PROJECT-B focal now passes in Root51d. This qualifies
the exercised Graphit registration/selection path, not installed vendor or
native platform behavior.

## Limits, errors and lifecycle

The reader uses captured content/instruction/output bounds and one finite
Original. Its deadline starts before waiting for the existing Source jobs lock;
the blocking worker retains that same deadline/cancellation in its
RequestContext and remains owned by the existing JoinSet. Recipient
cancellation, service stop and deadline prevent successful delivery, cancel
the operation and retain normal joined shutdown.

Invalid input/selection, missing or unsupported origin/bearer, denied
authentication/permission, unavailable configuration, changed capture/override,
unknown reference, render/output bounds and operation failure use the existing
finite tool-error boundary. No useful partial payload is successful. Final
checks retain caller, admission, relevant configuration, physical installation/
PROJECT, live selection and Original. These cooperative checks promise neither
hard RSS bounds nor atomic Auth/configuration/filesystem observation.

## Source and qualification

[Reader source](../../crates/graphit-app/src/source_server/module_skill.rs) owns
the contract; [focused tests](../../crates/graphit-app/src/source_server/module_skill/tests.rs)
contain actual Root HTTP/RMCP, proxy and cancellation scenarios.
[Source composition](../../crates/graphit-app/src/source_server.rs) and
[Root proxy](../../crates/graphit-app/src/mcp_source.rs) hold the coordinator's
integrated registration/dispatch and omission-preserving selection.

Written cases cover current two-PROJECT settings, disabled guidance, neutral
Global, normalization/exact references, authority/input denials, observed
overrides, replacement/absence and cancellation with retained worker, sibling
read and shutdown join. The Local case constructs actual AuthStore,
ProviderConfig and Local ProfileData in a temporary installation: Global and
PROJECT AST success are expected, with Memory denied. It neither reads nor
changes personal profiles. The proxy case preserves omission and an explicit
other PROJECT despite its own different captured default.

The latest integrated delta compiled in Root51c (`../../.build/rust-records-data/knowledge-index-source/auth-web-root-emit51c.result.json`; historical local artifact unavailable):
code 0, null signal and 232231 ms wall time. Its
stdout (`../../.build/rust-records-data/knowledge-index-source/auth-web-root-emit51c.stdout.log`; historical local artifact unavailable)
is empty; stderr (`../../.build/rust-records-data/knowledge-index-source/auth-web-root-emit51c.stderr.log`; historical local artifact unavailable)
contains 83578 bytes of compilation output. This is compilation evidence,
not strict qualification or a benchmark.

The initial reader compilation passed in Root50p (411577 ms). It predates
the integrated Brand correction and does not qualify compilation of that
latest delta. Compilation cost is not operation latency. The actual Root50v stdout (`../../.build/rust-records-data/knowledge-index-source/root-module-reader50v.stdout.log`; historical local artifact unavailable),
result (`../../.build/rust-records-data/knowledge-index-source/root-module-reader50v.result.json`; historical local artifact unavailable)
and stderr (`../../.build/rust-records-data/knowledge-index-source/root-module-reader50v.stderr.log`; historical local artifact unavailable)
record a mixed group: code 101, 546 ms wall time, four PASS, one FAIL and zero
ignored cases. Its four passing cases are:

- `observed_template_rejects_replacement_and_absent_override_creation`
- `root_cancellation_joins_original_worker_and_preserves_sibling_read`
- `root_local_reads_guidance_with_actual_temporary_profile`
- `root_proxy_preserves_global_omission_and_explicit_other_project`

Root50x additionally passed
`root_authority_exact_reference_and_observed_override_fail_closed`
with code 0, 285 ms wall time, zero ignored cases and empty stderr:
stdout (`../../.build/rust-records-data/knowledge-index-source/root-module-authority50x.stdout.log`; historical local artifact unavailable),
result (`../../.build/rust-records-data/knowledge-index-source/root-module-authority50x.result.json`; historical local artifact unavailable)
and stderr (`../../.build/rust-records-data/knowledge-index-source/root-module-authority50x.stderr.log`; historical local artifact unavailable).
These five functional passes retain their executed scopes.

`root_catalog_dispatch_current_project_settings_and_neutral_global` failed
with HTTP 413: its 70 kB fixture exceeded the existing 65536-byte HTTP cap
before reaching the intended scalar check. The integrated correction reduces
that value to 5 kB, still above the 4096-byte scalar limit, and asserts the
specific finite error. It changes neither limits nor timeout and retains the
negative oracle. Root51d case 0 now qualifies that repaired case; the
historical Root50v group remains failed.

Root51d records two separately selected exact cases. The
aggregate result (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.result.json`; historical local artifact unavailable)
is code 0, null signal and 680 ms wall time; its
cases result (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.cases-result.json`; historical local artifact unavailable)
and aggregate stdout (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.stdout.log`; historical local artifact unavailable)
identify both executions. Aggregate stderr (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.stderr.log`; historical local artifact unavailable)
is empty.

| Exact selector | Result and wall time | Test runtime | Actual per-case logs |
|---|---|---|---|
| `root_catalog_dispatch_current_project_settings_and_neutral_global` | code 0; 1 PASS, 0 failed/ignored; 403 ms | 0.38 s | stdout (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.0.stdout.log`; historical local artifact unavailable), empty stderr (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.0.stderr.log`; historical local artifact unavailable) |
| `root_alternate_brand_catalog_dispatch_and_proxy_keep_global_neutral` | code 0; 1 PASS, 0 failed/ignored; 236 ms | 0.22 s | stdout (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.1.stdout.log`; historical local artifact unavailable), empty stderr (`../../.build/rust-records-data/knowledge-index-source/root-pending51d.1.stderr.log`; historical local artifact unavailable) |

The affected strict51g result (`../../.build/rust-records-data/knowledge-index-source/auth-app-root-strict51g.result.json`; historical local artifact unavailable)
is a historical FAIL: code 101, null signal and 178164 ms wall time.
Stdout (`../../.build/rust-records-data/knowledge-index-source/auth-app-root-strict51g.stdout.log`; historical local artifact unavailable)
is empty; stderr (`../../.build/rust-records-data/knowledge-index-source/auth-app-root-strict51g.stderr.log`; historical local artifact unavailable)
contains 85624 bytes and reports one Clippy `collapsible_if` error at
`source_server/module_skill.rs:468`. The coordinator applied only the equivalent
`if let` chain for that live-selection check, preserving its condition, error,
authority, Original and cancellation behavior. Scoped rustfmt/check passed
with `skip_children=true`, and the coordinator explicitly retains all seven
functional proofs without replay. Formatting did not qualify strict; the
affected reexecution51z below provides the actual passing result.

The affected retry now passes in
result51z (`../../.build/rust-records-data/knowledge-index-source/integrated-auth-app-data-core-fs-strict51z.result.json`; historical local artifact unavailable):
code 0, null signal and 43859 ms wall time.
Stderr (`../../.build/rust-records-data/knowledge-index-source/integrated-auth-app-data-core-fs-strict51z.stderr.log`; historical local artifact unavailable)
contains 84824 bytes, no error lines and the dev-profile finish at 43.74 s.
No stdout artifact is asserted. The coordinator's invocation was:

```text
cargo xtask cargo clippy --locked --offline -p graphit-auth -p graphit-app -p graphit-data -p graphit-core -p graphit-fs --lib -- -D warnings
cargo xtask cargo clippy --locked --offline -p graphit-app --test integration -- -D warnings
```

This qualifies those affected packages/targets with warnings denied, not the
whole workspace or another OS/vendor runtime. It resolves the strict retry
after the minimal 51g correction; 51g remains a historical failed execution.

The coordinator confirmed unique selection before each `--exact` execution;
no separate list-log artifact is asserted. These two cases plus the five
preserved passes make seven distinct functional focals, with the affected
strict scope now passing in 51z. Broader 0257/9bd4 scope remains with its
existing owners and coordinator; these results create no new Runtime or
platform qualification campaign. No fixture proves installed vendor behavior,
model perception or native Windows/macOS runtime, and this page completes no
parent migration or session.
