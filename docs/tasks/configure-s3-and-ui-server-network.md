---
title: Configure optional S3 credentials and UI server network access
status: in-progress
created: 2026-08-24
updated: 2026-08-24
tags: [setup, s3, configuration, ui-server, cors]
---

# Configure optional S3 credentials and UI server network access

## Objective

Extend the interactive setup so operators may provide S3 credentials that are stored in the global configuration, while preserving the existing AWS credential-provider chain when either credential is omitted and explaining that fallback in the prompt. Add project-or-global configuration for the UI server bind host and allowed CORS origins, with `0.0.0.0` as the new default bind host and the existing localhost-only origin policy as the secure default.

The implementation will reuse the existing layered configuration system rather than add sidecar files or environment-only behavior. This keeps global and project overrides consistent with the rest of the CLI, makes setup-provided S3 credentials available to all projects, and preserves the deliberate security hardening that rejects non-local origins unless the operator explicitly opts in.

## Plan & Task Breakdown

- [x] **T1 — Map the setup, config, and UI-server paths** — Spec: identify the setup prompts, global config persistence, S3 option construction, UI bind call, and every UI-server CORS middleware; done means callers, dependencies, and existing tests are known before editing.
- [x] **T2 — Add optional global S3 credentials to setup** — Spec: prompt for access key and secret without requiring either; persist a complete pair globally; when incomplete or blank, retain the current provider chain and show a disclaimer; never print the secret.
- [x] **T3 — Add layered UI network configuration** — Spec: add project/global keys for bind host and allowed origins; default host is `0.0.0.0`; default origins remain localhost-only; explicit configured origins replace the default allowlist consistently across all UI endpoints.
- [x] **T4 — Add regression coverage and user documentation** — Spec: cover setup persistence/fallback, precedence/defaults, bind address, allowed and rejected origins; update the relevant configuration/setup documentation.
- [x] **T5 — Verify and close the task** — Spec: run focused tests and the proportionate repository checks, record results, reflect on the change, persist durable knowledge, and synchronize all Graphit indexes once before completion.
- [x] **T6 — Audit and complete the public documentation surface** — Spec: search the knowledge wiki for both the new keys/behaviors and the superseded Git/localhost assumptions; follow cross-references; update the operator guide, configuration reference, architecture/integration material, README/documentation index, ADR, and task log wherever they are affected. Done means a user can configure and secure the feature without reading source code or this conversation.
- [x] **T7 — Validate documentation integrity and close again** — Spec: verify examples against the implemented CLI syntax, run documentation/link checks and `graphit_knowledge_lint`, update durable memory, and issue one final all-module sync before reporting completion.
- [x] **T8 — Correct the white-label release build contract** — Spec: remove the obsolete `DEFAULT_HUB_REPO` release input, keep the current S3 brand defaults empty unless an operator supplies them, and restore the accidentally removed native Windows recipe that the release job invokes, without introducing credentials or reintroducing the unsupported cross-build.
- [x] **T9 — Validate release wiring and retire the backlog item** — Spec: add or run a deterministic check that proves the workflow passes only supported variables, validate the affected build path, update documentation and memory, remove the resolved backlog item, and synchronize all indexes before reporting completion.
- [x] **T10 — Commit the complete workspace on `main`** — Spec: inspect the complete working tree, confirm or switch safely to `main`, stage every tracked and untracked change exactly as requested, review the staged summary for accidental secrets or generated junk, commit with an English conventional-commit message, and report the resulting commit hash.

## Implementation Details

Structural exploration established the following implementation seams:

- `cmd/graphit/commands/setup.go:newSetupCmd` owns all interactive global setup prompts and verifies S3 through `verifyHubBucket`.
- `internal/config/hub_s3.go:S3Config` is the single resolved Hub/S3 value passed to AWS SDK, LanceDB, and LadybugDB consumers.
- `internal/s3store/store.go:New` loads the AWS default provider chain; it will add a static provider only when a complete configured pair exists.
- `internal/lancestore/config.go:storageOptions` renders LanceDB/object_store settings and must include the two credential keys only for a complete pair.
- `internal/ast/ladybug.go:prepareRemoteAccessLocked` currently reads only AWS environment variables for LadybugDB; configured values must win while the existing environment fallback remains.
- `internal/uiserver/unified_server.go:NewUnifiedServer` has the project directory needed to resolve layered UI settings. Its `Start` method owns the listener and wraps the entire mux in `hub.CorsWrap`, so one configured outer CORS wrapper covers Hub, AST, wiki, live-search, daemon/dream, health, readiness, and static UI endpoints.
- `internal/netutil/port.go` currently binds `":<port>"`, which already means all interfaces but does not expose an explicit host setting. Host-aware variants can preserve existing callers while making the UI choice testable.
- `internal/uiserver/unified_server.go:handleUI` injects `localhost` as the browser API base. Remote clients would call their own localhost; the unified page must instead use the existing same-origin `/api` fallback.

The Hub registry has no installed or available knowledge artifact for AWS SDK, LanceDB, object_store, or LadybugDB. Official AWS SDK documentation confirms `config.WithCredentialsProvider(credentials.NewStaticCredentialsProvider(...))`; official `object_store` documentation confirms the `access_key_id` and `secret_access_key` keys accepted by LanceDB's storage options.

## Use Cases

### UC-01: Configure explicit S3 credentials during setup
- **Actor**: Operator running the interactive setup.
- **Preconditions**: Interactive input is available and the global configuration directory is writable.
- **Main Flow**:
  1. Setup asks for the S3 access key and secret key.
  2. The operator supplies both values.
  3. Setup stores both values in global configuration without echoing the secret.
- **Alternative Flows**:
  - The operator leaves the credentials blank and setup retains the existing AWS credential-provider chain.
- **Error Scenarios**:
  - Only one half of the credential pair is supplied; setup does not persist a partial credential and explains the fallback.
  - Global configuration persistence fails; setup reports the error and does not claim success.
- **Postconditions**: Either a complete explicit credential pair exists globally or no explicit pair exists and the provider chain remains active.
- **Affected Files**: `cmd/graphit/commands/setup.go`, `internal/config/hub_s3.go`, `internal/s3store/store.go`, `internal/lancestore/config.go`, `internal/ast/ladybug.go`, and their tests.

### UC-02: Configure UI server bind host
- **Actor**: Operator or project maintainer.
- **Preconditions**: The UI server command can resolve layered configuration.
- **Main Flow**:
  1. The operator sets the bind-host key globally or for a project.
  2. The UI server resolves the effective value using normal config precedence.
  3. The server listens on that host.
- **Alternative Flows**:
  - With no override, the server listens on `0.0.0.0`.
- **Error Scenarios**:
  - An invalid host causes startup to fail with contextual diagnostics from the listener.
- **Postconditions**: The UI server is bound to the configured or default host.
- **Affected Files**: `internal/config/ui_server.go`, `internal/netutil/port.go`, `internal/uiserver/unified_server.go`, and their tests.

### UC-03: Override allowed UI origins
- **Actor**: Operator or project maintainer.
- **Preconditions**: A browser request includes an `Origin` header and the UI server can resolve layered configuration.
- **Main Flow**:
  1. The operator sets one or more allowed origins globally or for a project.
  2. CORS middleware compares the request origin with the effective configured set.
  3. Matching origins receive `Access-Control-Allow-Origin`; other origins do not.
- **Alternative Flows**:
  - With no override, only empty/same-origin and localhost loopback origins remain accepted.
- **Error Scenarios**:
  - A malformed or non-matching origin is rejected without reflecting it in the response.
- **Postconditions**: Browser access follows the explicit override or the secure localhost default.
- **Affected Files**: `internal/config/ui_server.go`, `internal/hub/ui_server.go`, `internal/uiserver/unified_server.go`, and their tests.

## Test Cases & Acceptance Criteria

### Feature: Optional S3 credentials in setup
Ref: UC-01

#### Scenario: Complete credential pair is stored globally
```gherkin
Given interactive setup has a writable isolated global configuration
When the operator enters access key "test-access" and secret key "test-secret"
Then both values are stored in global configuration
  And the secret is not printed in setup output
```

#### Scenario: Blank credentials preserve the provider chain
```gherkin
Given interactive setup has no explicit S3 credentials
When the operator leaves both credential prompts blank
Then no explicit credential pair is stored
  And setup explains that environment, profile, instance-role, or equivalent provider-chain credentials will be used
```

#### Scenario: Partial credentials are not persisted
```gherkin
Given interactive setup has no explicit S3 credentials
When the operator enters only access key "test-access"
Then neither credential is persisted
  And setup explains that a complete pair is required and the provider chain remains active
```

### Feature: UI server bind host
Ref: UC-02

#### Scenario: Default bind host is externally reachable
```gherkin
Given neither global nor project configuration sets the UI bind host
When the UI server resolves its listen address
Then the host is "0.0.0.0"
```

#### Scenario: Project bind host overrides global bind host
```gherkin
Given global configuration sets the bind host to "0.0.0.0"
  And project configuration sets the bind host to "127.0.0.1"
When the UI server resolves its listen address
Then the host is "127.0.0.1"
```

### Feature: UI allowed-origin override
Ref: UC-03

#### Scenario: Localhost remains allowed by default
```gherkin
Given no allowed-origin override exists
When a request has origin "http://localhost:3000"
Then the response reflects "http://localhost:3000" as allowed
```

#### Scenario: Explicit origin replaces the localhost allowlist
```gherkin
Given the effective allowed-origin configuration contains "https://ui.example.test"
When a request has origin "https://ui.example.test"
Then the response reflects "https://ui.example.test" as allowed
```

#### Scenario: Unconfigured origin is rejected
```gherkin
Given the effective allowed-origin configuration contains "https://ui.example.test"
When a request has origin "https://evil.example.test"
Then the response omits Access-Control-Allow-Origin
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/configure-s3-and-ui-server-network.md` | Created | Open the task with objective, rationale, use cases, acceptance criteria, and execution plan. |
| `cmd/graphit/commands/setup.go` | Updated | Prompt for an optional S3 credential pair, explain provider-chain fallback, and improve verification diagnostics. |
| `cmd/graphit/commands/lifecycle.go` | Updated | Redact the configured S3 secret from config get/list output. |
| `cmd/graphit/commands/setup_credentials_test.go` | Created | Verify complete-pair persistence and blank/partial fallback behavior. |
| `cmd/graphit/commands/config_secret_test.go` | Created | Verify secret redaction. |
| `internal/config/hub_s3.go` | Updated | Resolve optional S3 credentials and persist/remove the global pair together. |
| `internal/config/ui_server.go` | Created | Resolve layered UI host and allowed-origin settings. |
| `internal/config/ui_server_test.go` | Created | Cover defaults, global/project precedence, parsing, and pair semantics. |
| `internal/s3store/store.go` | Updated | Inject a static AWS provider only for a complete configured pair. |
| `internal/s3store/store_test.go` | Updated | Verify configured credentials win over the environment provider. |
| `internal/testsupport/fakes3.go` | Updated | Expose the signed request access key to the S3 regression test. |
| `internal/lancestore/config.go` | Updated | Pass complete credentials to object_store/LanceDB. |
| `internal/lancestore/config_test.go` | Created | Verify complete pairs are emitted and partial pairs omitted. |
| `internal/ast/ladybug.go` | Updated | Prefer configured credentials while preserving the existing AWS environment fallback. |
| `internal/ast/ladybug_s3_config_test.go` | Created | Cover configured and environment credential resolution. |
| `internal/netutil/port.go` | Updated | Add host-aware port discovery/listener helpers without breaking existing callers. |
| `internal/netutil/netutil_test.go` | Updated | Verify exact-host binding. |
| `internal/hub/ui_server.go` | Updated | Add configurable exact-origin CORS policy while retaining the localhost default. |
| `internal/hub/ui_server_test.go` | Updated | Cover replacement semantics, rejected origins, and explicit wildcard. |
| `internal/uiserver/unified_server.go` | Updated | Resolve layered UI settings, bind explicitly, apply CORS to the full mux, and use same-origin `/api`. |
| `internal/uiserver/unified_server_network_test.go` | Created | Verify remote-safe UI bootstrap behavior. |
| `docs/specs/config_module.md` | Updated | Document keys, precedence, examples, fallback, storage caveat, and CORS risk. |
| `docs/decisions/2026-08-24-credenciais-s3-e-rede-do-ui-server.md` | Created | Preserve the authentication and network-security decisions. |
| `go.mod` | Updated | Promote the AWS credentials package to a direct dependency. |
| `docs/tasks/backlog/testpipelinewritesimportnodestothegraph-can-query-empty-schema-under-race.md` | Created | Defer investigation of the independently observed AST race-test flake. |
| `docs/guides/s3-and-ui-network.md` | Created | Canonical operator guide for S3 authentication and UI network security. |
| `README.md` | Updated | Replace obsolete Git setup/collaboration claims and expose the new network guide. |
| `docs/README.md` | Updated | Add the operator guide and correct Hub/Memory descriptions. |
| `docs/guides/getting_started.md` | Updated | Explain optional credentials, provider-chain fallback, and safe UI exposure. |
| `docs/guides/cli_reference.md` | Updated | Correct setup/config/UI behavior and remove the nonexistent UI port flag. |
| `docs/guides/user_manual.md` | Updated | Replace Git-backed memory and Hub rule distribution with S3 prefixes. |
| `docs/guides/private_brand_customization.md` | Rewritten | Document current S3 brand defaults, private deployment, credential handling, and UI hardening. |
| `docs/guides/troubleshooting.md` | Updated | Replace Git remote diagnostics with S3/provider-chain checks and add UI bind/CORS diagnostics. |
| `docs/architecture/architecture_overview.md` | Updated | Describe local plus S3-backed persistence and the unauthenticated UI boundary. |
| `docs/specs/hub-s3-object-layout.md` | Updated | Correct the credential contract and setup failure behavior. |
| `docs/specs/hub_collaboration.md` | Rewritten | Replace the removed Git store model with the S3 registry/collaboration contract. |
| `docs/specs/memory_module.md` | Rewritten | Document local raw memory, S3 merge/publish semantics, history, shards, and local-only mode. |
| `docs/specs/rule_override.md` | Updated | Replace Hub main-branch terminology with the S3 `rules/` prefix. |
| `docs/specs/ui_dashboard.md` | Updated | Document UI bind, origin policy, same-origin API, and lack of authentication. |
| `docs/tasks/security-fix-cors-headers.md` | Updated | Add an evolution note preserving the historical decision while linking the new override. |
| `docs/site/index.html` | Updated | Bring the static site collaboration and architecture copy from Git to S3. |
| `Makefile` | Updated | Restore the native MSYS2 Windows release recipe, including the required search/runtime libraries, without restoring the unsupported cross-build. |
| `.github/workflows/release.yml` | Updated | Remove the obsolete `DEFAULT_HUB_REPO` argument and rely on the current empty S3 defaults. |
| `docs/tasks/backlog/release-workflow-ainda-passa-default-hub-repo-removido.md` | Removed | Resolve the release mismatch in this task after the scope correction. |

## Trade-offs & Decisions

- The localhost-only CORS behavior remains the default because it was introduced as a deliberate protection against hostile websites reading locally served project data.
- Explicit S3 credentials are stored only as a complete pair; partial credentials would create ambiguous authentication failures and unnecessarily suppress a working provider chain.
- The new UI settings use the existing layered configuration mechanism so project values can override global values without a parallel configuration surface.

## Technical Debt

- `TestPipelineWritesImportNodesToTheGraph` observed an empty/mid-rebuild graph in one full race-enabled run, then passed 3/3 in isolation and in the complete retry. Investigation is recorded in `docs/tasks/backlog/testpipelinewritesimportnodestothegraph-can-query-empty-schema-under-race.md`; its assertion was not weakened.
- Resolved in T8: `.github/workflows/release.yml` no longer passes the removed `DEFAULT_HUB_REPO`, and the native Windows target invoked by the release was restored without reintroducing cross-compilation.

## System Knowledge

- Setup has previously exposed integration defects that package tests missed, so verification must include the assembled command behavior where practical.
- S3-compatible endpoints such as MinIO may additionally require path-style requests and HTTP allowance; the credential change must not alter those existing options.
- The current localhost CORS allowlist was a deliberate security hardening across three server packages, not an accidental limitation.
- The unified server already wraps its complete mux in Hub CORS middleware; per-handler wiki CORS is redundant but does not remove a header set by the outer wrapper.
- Although the current listener uses `":port"` (all interfaces), the browser bootstrap hardcodes a localhost API base, which prevents a remote browser from using the server correctly.
- Public releases intentionally leave `DEFAULT_HUB_BUCKET`, `DEFAULT_HUB_REGION`, and `DEFAULT_HUB_ENDPOINT` empty; private distributors may supply them, but credentials never belong in linker flags or workflows.
- A `.PHONY` target with no recipe exits successfully and can hide a deleted release build. Release validation must inspect the expanded target, not only YAML syntax or variable names.

## Progress Log

### 2026-08-24
- T10 is complete: the full audited workspace snapshot was committed on `main` with the English conventional message `feat(config): add S3 credentials and UI network controls`. This closure entry is included by amending that same commit; the final hash is reported to the Engenheiro after verification.
- T10 audit is complete: the repository is already on `main`; all 46 current paths are staged; there are no untracked files, whitespace errors, high-confidence credential/private-key patterns, or staged files larger than 1 MiB. The explicitly requested "everything" includes the existing `.graphit/dream/dream_last_seen.json` marker. The commit message follows the repository's English conventional-commit rule.
- The Engenheiro requested that every current workspace change be committed directly on `main`. Reopened the task as T10; the commit will include all tracked and untracked files after a final staged-content audit, without discarding or rewriting existing work.
- Post-task reflection reviewed all 214 project memories, the resolved improvement rules, all Dream reports, the backlog, and Dream status. The scope-correction memory was updated with the accidentally removed native-target finding; no other memory became stale, and this project-specific release repair does not warrant a reusable IDE/Hub artifact.
- The resolved release item was removed from the improvement backlog. Thirteen unrelated items remain untouched; the daemon is running but Dream reports `inactive`, so no automatic-processing promise is made. `graphit_knowledge_lint` completed with 797 broad pre-existing findings; the new documentation adds no filesystem link and `git diff --check` remains clean.
- T9 is complete. The mandated all-module synchronization is intentionally the final tool operation before reporting completion.
- Completed T8: removed `DEFAULT_HUB_REPO` from the Windows release job and restored only the native `build-windows-native` recipe from the pre-`911d1fc` contract, preserving the decision that every release is built on its own platform with LanceDB search enabled.
- Deterministic release checks pass: `release.yml` parses with Ruby/Psych; the obsolete variable is absent; the workflow still invokes the target; `make -n` expands core, MCP, launcher, LanceDB, LadybugDB, ONNX, AST-query, and current S3 linker inputs; and `git diff --check` is clean. `actionlint` is not installed in this environment, so no claim is made for that additional linter.
- Updated the private-brand deployment guide with the public empty-default policy, the three supported S3 build variables, the obsolete variable warning, and the native Windows release invariant. Updated the durable correction memory and removed the resolved improvement backlog item.
- Validation of the release job exposed a second defect in the same path: `build-windows-native` remained declared as `.PHONY` and referenced by the workflow/install target, but commit `911d1fc` accidentally removed its recipe together with the intentionally removed Windows cross-build. Expanded T8 to restore only the native recipe; the unsupported cross-build remains absent.
- The Engenheiro corrected the earlier scope decision: the stale Makefile/release-workflow mismatch is directly related to the S3/UI work and must be fixed in this same task rather than deferred. Reopened the task as T8/T9 and recorded the correction in project memory.
- Opened the task after consulting project memory, the knowledge wiki, prior CORS hardening documentation, and the resolved improvement rules.
- Confirmed that blank setup credentials must preserve the current AWS provider chain and that configured origins must be an explicit override of the secure localhost default.
- Mapped exact code entities, callers, test reachability, and external API contracts through the AST graph, project wiki, and official provider documentation.
- Chosen implementation: complete-pair credential semantics, atomic global persistence from setup, explicit credential injection in all three S3 consumers, `ui.host` plus comma-separated `ui.allowed_origins`, host-aware listener helpers, and same-origin browser API calls.
- Implemented T2 and T3 plus focused regression tests. The config, S3 store, LanceDB config, netutil, unified UI, and command packages pass their focused tests.
- The broader focused run exposed two pre-existing/environmental failures outside this change: `internal/ast/search_hybrid_floor_test.go` references undefined `hasName`, and a Hub integration test requires the `lancedb` build tag/native dependency. These are being checked against project memory before choosing the proportionate verification command.
- Re-ran the affected AST and Hub packages through the supported `-tags lancedb` path; both pass. Added explicit global-to-project precedence coverage for both UI keys.
- Documented the S3 credential fallback, secret-at-rest caveat, UI bind/origin examples, wildcard risk, and same-origin browser behavior in the config specification. Recorded the security/operational trade-off in an accepted decision.
- The full `make test` gate passed every package except one AST race-enabled case: `TestPipelineWritesImportNodesToTheGraph` observed no `Import` table and reported an empty/mid-rebuild graph. The affected AST package had already passed in the focused tagged run; a project-memory lookup for this exact flake timed out under post-suite index load.
- Repeated the exact case with `-race -tags lancedb -count=3`; it passed 3/3. A second full `make test` then passed, including the race-enabled project packages and generated-parser pass.
- `go build ./...` and `git diff --check` pass. The untagged focused AST test command remains unsupported for pre-existing tag-layout reasons; all affected packages pass together through the supported LanceDB-tagged path.
- Reflection recorded one durable project decision memory and one deferred flake investigation; no reusable Hub artifact was warranted because the change is project-specific.
- Final security review aligned LadybugDB with the AWS SDK: a configured static pair no longer inherits an unrelated `AWS_SESSION_TOKEN`; the token is used only with the environment fallback.
- Re-ran the Ladybug credential test with `-race`, every other affected package with `-tags lancedb`, `go build ./...`, and `git diff --check`; all pass after the security adjustment.
- Implementation and verification are complete; the mandated all-module synchronization is issued as the final close-out operation immediately before reporting completion.
- The Engenheiro then requested that **everything** be documented. Reopened the completed task rather than treating this as a separate report, re-read the task log and mandatory skills, and recorded the correction in project memory.
- The first wiki audit already exposed one stale surface: the configuration specification still contains a historical `Hub & Memory Repository Paths` section describing removed `hub.repo`/`memory.repo` Git behavior. The broader operator/architecture/reference audit is now T6; documentation will not be called complete until that obsolete material and all affected public entry points are reconciled.
- Added `docs/guides/s3-and-ui-network.md` as the canonical operator guide. It documents precedence and scope, every S3/UI key and environment variable, setup's complete-pair behavior, AWS provider-chain fallback, plain-text global credential storage and redaction, listener exposure, exact-origin replacement semantics, wildcard risk, same-origin API behavior, the lack of UI authentication, deployment profiles, and troubleshooting.
- Reconciled every current public entry point found by the wiki audit: root/docs READMEs, getting started, CLI and user manuals, private deployment, troubleshooting, architecture, config/Hub/Memory/rule/UI specifications, ADR, historical CORS task, and the non-indexed static site. Historical migration task logs remain historical evidence rather than being rewritten as current guidance.
- The AST graph confirmed the current brand fields and UI command surface; because Makefiles/workflows are not represented structurally, the documented fallback checked build variables textually and exposed one stale release setting, now recorded in the improvement backlog.
- T6 is complete. Post-edit wiki searches confirm the current README, CLI reference, troubleshooting guide, rule specification, and architecture no longer present the removed Git backend or nonexistent UI port flag as current behavior. Historical task/change records were deliberately retained.
- Documentation validation so far: `git diff --check` passes; every local Markdown target in the changed documentation exists; the static site has no residual Hub/Memory Git-backend language. `graphit_knowledge_lint` ran successfully and reported the repository's broad pre-existing wiki-link/frontmatter debt (794 findings), including normal relative file links that the generated-wiki linter does not resolve; no changed-file filesystem link is broken.
- Post-task reflection listed the full memory store, improvement backlog, and Dream status. It preserved the newly recorded documentation-completeness correction, marked two pre-S3 Hub memories as superseded instead of allowing them to conflict with current guidance, and kept the stale release variable as an explicit backlog item. No reusable Hub artifact was warranted for project-specific operator documentation.
- T7 is complete except for the mandated final all-module synchronization, which is intentionally the last tool operation before reporting completion.
