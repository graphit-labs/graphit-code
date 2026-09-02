---
title: Deterministic session-start memory hooks for all adapters
status: completed
created: 2026-09-02
updated: 2026-09-02
tags: [adapters, hooks, session-start, memory, mandates]
---

# Deterministic session-start memory hooks for all adapters

## Objective

Make memory initialization deterministic in every supported adapter. Adapter installation must install a hook that captures session start and requires the LLM to execute the ordered memory protocol: consume the complete mandatory set first, then run a contextual search excluding mandatory memories. Because the hook becomes the universal enforcement point, generated mandates must stop carrying the session-start protocol.

The approach is to inventory the adapter abstraction and its existing hook capabilities through the AST graph, implement each IDE's project-local hook and MCP configuration inside its concrete adapter, and add cross-adapter tests that prove installation and generated content. This places the lifecycle guarantee in executable adapter configuration instead of relying on probabilistic prose repeated in mandates.

## Plan & Task Breakdown

- [x] **T1 — Inventory adapters and hook installation paths** — Spec: map every adapter implementation, its install/sync entry points, existing hook support, callers, dependents, and tests. Done means the complete adapter matrix and the shared extension point are known before edits; no adapter may be omitted.
- [x] **T2 — Define the deterministic session-start hook contract** — Spec: establish the hook payload and ordered memory calls (`graphit_memory_mandatory`, then contextual `graphit_memory_search` with mandatory results excluded), preserving each adapter's native hook format and idempotent installation semantics.
- [x] **T3 — Implement hook installation for every adapter** — Spec: modify the adapter layer and any adapter-specific assets so all supported adapters receive the session-start hook during Graphit installation/sync. Done means repeated installation is idempotent and existing user hook configuration is preserved.
- [x] **T4 — Remove deterministic startup prose from mandates** — Spec: update the memory mandate generator and affected assertions so the mandate no longer duplicates behavior now guaranteed by hooks; keep non-startup memory triggers and operational guidance intact.
- [x] **T5 — Add and run coverage** — Spec: add focused unit/integration tests for every adapter, hook content/order, idempotency, preservation of user configuration, and mandate output; run the relevant package suite and broader validation proportional to the change.
- [x] **T6 — Update durable documentation and synchronize indexes** — Spec: update the relevant feature/architecture documentation plus this log with exact files, use cases, decisions, results, and remaining debt; run one final `graphit_sync` so AST, wiki, and memory indexes reflect the completed change.
- [x] **T7 — Restore per-adapter hook ownership** — Spec: remove the IDE-name switch and hook lifecycle from `FolderBasedAdapter`; make each concrete adapter install and remove its own project-local hook while retaining only format-agnostic helpers and the shared protocol.
- [x] **T8 — Revalidate corrected architecture** — Spec: add structural assertions for project-local paths and per-adapter ownership, rerun focused and complete tests, update the durable record, and synchronize indexes.
- [x] **T9 — Prefer project-scoped MCP installation** — Spec: move each supported IDE's MCP target into the project and ensure both the Graphit MCP and every Hub-installed MCP are reconciled and removed by the owning adapter.
- [x] **T10 — Correct adapter path metadata** — Spec: verify Antigravity and Gemini project MCP support against current sources, correct the scope matrix, and place every native hook path in `FolderConfig` beside the adapter's other paths while retaining hook behavior in the concrete adapter.
- [x] **T11 — Remove cross-project MCP ownership metadata** — Spec: because all MCP files are now project-local, remove `_graphitManagedMcpKeys` and the project-claim model; preserve user-owned MCP entries while installing, refreshing, and removing only the MCP names managed by the current project's adapter.
- [x] **T12 — Drop legacy MCP migration** — Spec: remove all code, tests, and documentation that read or migrate `_graphitManagedMcpKeys`; development builds have no backward-compatibility contract, so reconciliation starts only from the current project-local manifest.
- [x] **T13 — Use only OpenCode's native MCP key** — Spec: make the OpenCode adapter write, refresh, and remove servers only through the native `mcp` object; never emit the generic `mcpServers` compatibility copy, while preserving unrelated OpenCode configuration and user-owned MCP entries.
- [x] **T14 — Make manual session-hook invocation non-blocking** — Spec: reproduce `graphit _session-hook --format first-invocation` on an interactive terminal, remove the indefinite stdin wait without breaking piped native hook payloads, and cover both terminal/no-input and JSON-input execution.

## Implementation Details

Every IDE has one concrete adapter file containing configuration, `Sync`/`Remove`, IDE-specific behavior, and hook ownership. `FolderBasedAdapter` retains only format-neutral artifact and standard `mcpServers` reconciliation.

Adapter matrix:

| Adapter | Native startup surface | Managed hook | MCP target |
|---|---|---|---|
| Claude | `SessionStart` command hook | `.claude/settings.json` | `.mcp.json` |
| Codex | `SessionStart` command hook | `.codex/hooks.json` | `.codex/config.toml` |
| Cursor | `sessionStart` command hook | `.cursor/hooks.json` | `.cursor/mcp.json` |
| Gemini | `SessionStart` command hook | `.gemini/settings.json` | `.gemini/settings.json` |
| Kiro | `SessionStart` agent hook | `.kiro/hooks/graphit-memory.json` | `.kiro/settings/mcp.json` |
| Antigravity | first `PreInvocation` (`invocationNum == 0`) because no `SessionStart` event exists | `.agents/hooks.json` | `.agents/mcp_config.json` |
| OpenCode | observe `session.created`, then inject at the first system-prompt transform for that `sessionID` | `.opencode/plugins/graphit-memory-session-start.js` | `opencode.json` |

The common semantic payload is an instruction injected before the first model response. It requires the model to call `graphit_memory_mandatory` first, then derive a query from the current request and call `graphit_memory_search` with `exclude_mandatory: true`, then read selected result pages with `graphit_wiki_source` before acting. Command-based adapters call a hidden Graphit hook renderer so no shell/runtime dependency beyond the already-installed Graphit executable is introduced.

Implementation now consists of:

- `internal/sessionhook`: the canonical protocol plus three format-neutral JSON renderers used by command-driven adapters.
- hidden `_session-hook --format <format>` CLI entry point: reads native hook input from stdin and writes only the selected output shape without knowing IDE names or starting the daemon.
- `antigravity.go`, `cursor.go`, `claude.go`, `kiro.go`, `codex.go`, `opencode.go`, and `gemini.go`: each owns the complete IDE adapter, including project-local hook and MCP lifecycle where supported.
- adapter-native reconcilers: merge arrays/objects without discarding user entries, replace both the legacy centralized command and the canonical adapter-owned hook on repeat sync, and remove only Graphit-owned entries.
- Hub MCP artifacts: flow through `DesiredMCPServers` for every adapter, including the custom Codex TOML and OpenCode JSON shapes.
- MCP reconciliation: writes no cross-project claim map into native IDE configuration. A manifest under the current project's `.graphit/runtime/cache/mcp` stores only the names written by each adapter, allowing stale/removal cleanup without touching user-owned servers. There is no legacy compatibility path while the project remains in development.
- the memory mandate: no longer contains the startup trigger or its ordered call sequence; the full protocol remains in the memory skill for reference and in the executable hook payload for enforcement.

## Use Cases

### UC-01: Install Graphit support in an adapter
- **Actor**: Graphit installer or sync process.
- **Preconditions**: A supported adapter is selected and its project configuration is writable.
- **Main Flow**:
  1. The installer resolves the adapter.
  2. The adapter installs its native session-start hook.
  3. The hook instructs the LLM to execute mandatory recall followed by contextual recall excluding mandatory memories.
- **Alternative Flows**:
  - Existing adapter configuration is merged without discarding user-owned hooks.
  - Repeated installation leaves one canonical Graphit hook.
- **Error Scenarios**:
  - Invalid or unwritable adapter configuration returns an actionable installation error without corrupting the existing file.
- **Postconditions**: The adapter deterministically invokes the Graphit memory initialization protocol at session start.
- **Affected Files**: To be identified by T1.

### UC-02: Generate project mandates after hook installation
- **Actor**: Graphit sync process.
- **Preconditions**: The memory module contributes its generated mandate trigger.
- **Main Flow**:
  1. Sync generates the adapter's mandate artifact.
  2. The mandate retains just-in-time memory guidance for later triggers.
  3. The mandate omits the deterministic session-start protocol owned by hooks.
- **Alternative Flows**:
  - Existing unrelated mandate sections remain unchanged.
- **Error Scenarios**:
  - A stale generated artifact is rewritten by the existing mandate freshness mechanism.
- **Postconditions**: Startup behavior has one enforcement point and mandates contain no duplicate startup protocol.
- **Affected Files**: To be identified by T1.

## Test Cases & Acceptance Criteria

### Feature: Deterministic adapter memory initialization
Ref: UC-01

#### Scenario: Every adapter installs the ordered session-start protocol
```gherkin
Given each supported adapter is installed into an isolated project configuration
When Graphit installs or synchronizes adapter support
Then each adapter contains exactly one Graphit session-start hook
  And the hook requires mandatory recall before contextual recall
  And the contextual recall excludes mandatory memories
```

#### Scenario: Reinstalling preserves user configuration
```gherkin
Given a supported adapter configuration contains a user-owned hook and one Graphit hook
When Graphit installation runs again
Then the user-owned hook remains unchanged
  And exactly one canonical Graphit session-start hook remains
```

#### Scenario: An invalid hook target fails safely
```gherkin
Given a supported adapter hook target cannot be parsed or written
When Graphit attempts to install the session-start hook
Then it returns an actionable error
  And the pre-existing adapter configuration remains intact
```

### Feature: Mandate responsibility after deterministic hooks
Ref: UC-02

#### Scenario: Generated memory mandate omits session-start protocol
```gherkin
Given deterministic memory initialization is installed by adapter hooks
When the memory mandate trigger is generated
Then it does not contain the session-start mandatory recall sequence
  And it retains the later memory activation rules and MCP-first constraints
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/deterministic-session-start-memory-hooks-for-all-adapters.md` | Created and updated | Open the task and record the adapter matrix and hook contract before code changes. |
| `internal/sessionhook/sessionhook.go` | Created | Define the canonical ordered protocol and render adapter-specific command hook output. |
| `internal/sessionhook/sessionhook_test.go` | Created | Prove call order, exclusion, page reads, adapter payloads, and Antigravity first-invocation behavior. |
| `cmd/graphit/commands/session_hook.go` | Created | Expose the renderer as a hidden, script-free hook command. |
| `cmd/graphit/commands/session_hook_test.go` | Created | Exercise the hidden renderer command. |
| `cmd/graphit/commands/root.go` | Updated | Register the hidden hook command and exclude it from daemon startup. |
| `internal/hub/adapters/ide/base.go` | Updated | Keep the shared base IDE-neutral, resolve project-relative MCP paths, and share canonical Hub MCP/server-state helpers. |
| `internal/hub/adapters/ide/adapters.go` | Updated | Retain only adapter lookup and shared managed-skill/artifact helpers. |
| `internal/hub/adapters/ide/{antigravity,cursor,claude,kiro,codex,opencode,gemini}.go` | Created | Concentrate each complete IDE adapter, including native project hook and narrowest-supported MCP lifecycle. |
| `internal/hub/adapters/ide/session_hooks.go` | Created | Provide format-neutral hook JSON/file reconciliation helpers without IDE routing. |
| `internal/hub/adapters/ide/session_hooks_test.go` | Created | Cover every adapter, idempotency, user configuration preservation, removal, and invalid JSON safety. |
| `internal/hub/adapters/ide/ide_test.go` | Updated | Prove each adapter uses the narrowest supported MCP scope and installs/removes MCPs contributed by Hub artifacts. |
| `internal/memory/rule.go` | Updated | Remove deterministic startup protocol from the generated mandate while retaining later memory triggers. |
| `internal/memory/rule_tools_test.go` | Updated | Assert the skill keeps reference guidance while the mandate delegates startup to hooks. |
| `docs/architecture/architecture_overview.md` | Updated | Document the deterministic adapter bootstrap and the two adapters that map it to their earliest model boundary. |

## Trade-offs & Decisions

- The ordered protocol is enforced through lifecycle hooks rather than generated prose because adapter execution is deterministic while instruction following is not.
- Adapter-native hook formats will be preserved behind a shared semantic contract; forcing one physical format across IDEs would break their configuration models.
- Command hooks invoke the Graphit executable itself to render a format selected by the owning adapter. This avoids shipping shell, Python, or Node helper scripts without centralizing IDE paths or lifecycle rules.
- Antigravity and OpenCode use their earliest model-boundary hooks as documented equivalents because neither exposes a native context-injecting `SessionStart` command hook.

## Technical Debt

- None identified yet.

## System Knowledge

- Mandatory memory is unconditional session context and independent from `important` memory.
- The session-start protocol has two ordered phases: `graphit_memory_mandatory`, then `graphit_memory_search` with `exclude_mandatory: true`.

## Progress Log

### 2026-09-02
- Opened the task log before source changes.
- Searched project memory and knowledge; confirmed the two-phase memory contract and the existing mandate generation context.
- Mapped all seven adapters and confirmed that their sync/remove lifecycle converges on `FolderBasedAdapter`.
- Verified each vendor's current hook schema and selected the native startup surface shown in the adapter matrix.
- Defined the shared ordered protocol and idempotent, user-preserving reconciliation strategy.
- Implemented the shared renderer, hidden CLI bridge, and native hook reconciliation for all seven adapters.
- Added shared sync/removal integration and atomic, idempotent writes that preserve user-owned configuration.
- Removed the session-start trigger and ordered sequence from the generated memory mandate while retaining the non-startup memory rules.
- Added focused coverage; the first relevant run passes for `internal/sessionhook`, `internal/hub/adapters/ide`, `internal/memory`, and `cmd/graphit/commands`.
- Updated the architecture overview with the adapter-owned startup invariant and cleanup boundary.
- Ran `git diff --check` and the complete `go test ./...` suite successfully.
- Updated the durable design memory with the exact adapter mapping, lifecycle boundaries, and mandate responsibility.
- Ran `graphit_sync`; AST, knowledge, and memory indexes completed successfully. A final sync after this completion marker leaves the task log itself current.
- User correction: hook artifacts must be project-local whenever supported, and each IDE adapter must own its own hook installation/removal instead of routing every IDE through one central switch. Reopened the task to refactor ownership without changing the ordered protocol.
- T7 in progress: removed hook routing and IDE identity from `FolderBasedAdapter`; the shared base now handles only folder artifacts and MCP lifecycle while hook ownership moves to each concrete adapter.
- T7 implemented: Antigravity, Cursor, Claude, Kiro, Codex, OpenCode, and Gemini now each own project-local hook paths plus sync/removal methods in adapter-specific files. The shared renderer selects neutral output formats instead of switching on IDE names.
- Replaced repeated concrete-type switches for folder-backed adapters with the common embedded-base contract so the new concrete adapter types retain skill and artifact path behavior.
- Focused tests pass after exercising hook creation through each adapter's public `Sync` lifecycle, idempotent reinstall, ordered memory protocol, user-config preservation, and adapter `Remove` cleanup.
- User clarified the desired physical organization: keep each hook with the complete IDE adapter rather than creating separate `*_hooks.go` files. The current repository actually had all concrete adapters in `adapters.go`, so T7 now includes splitting that monolith into one complete adapter file per IDE and leaving only registry/shared artifact helpers in `adapters.go`.
- User extended the same ownership rule to MCP installation, including MCP artifacts installed from the Hub: each adapter must prefer its IDE's project-scoped MCP configuration and own that configuration path/format; global configuration is a fallback only where project scope is unavailable.
- Split the former concrete-adapter monolith into seven complete IDE files; each now owns configuration, native integrations, hook lifecycle, and MCP lifecycle.
- Changed Claude, Cursor, Kiro, Codex, OpenCode, and Gemini to their documented project MCP targets; retained only Antigravity's documented global MCP target.
- Updated custom Codex and OpenCode reconciliation to include `DesiredMCPServers(installed)`, so Hub-installed MCP artifacts are installed, refreshed, and removed alongside Graphit's MCP without discarding user entries.
- Added migration coverage for legacy `_session-hook --adapter` commands and cross-adapter MCP scope/Hub installation coverage.
- Ran `git diff --check` and `go test ./...` successfully after the ownership and MCP-scope corrections.
- Ran final `graphit_sync` successfully, then updated the already-generated project hook artifacts from the legacy adapter selector to the new format selector because the long-lived MCP process still runs the pre-change binary.
- Ran the current source implementation with the stable launcher path to exercise real adapter sync. IDE adapter sync completed and generated project MCP targets (`.mcp.json`, `.codex/config.toml`, `.kiro/settings/mcp.json`, and `opencode.json`) plus migrated hook commands. Its optional in-process knowledge/vector phase reported the expected no-`lancedb` build warning; the MCP `graphit_sync` run had already completed those indexes successfully.
- Verified the generated Claude, Codex, and Antigravity hook commands use `--format`, and verified the generated Claude, Codex, Kiro, and OpenCode project MCP files contain the Graphit server; final `git diff --check` is clean.
- User questioned whether Antigravity supports project MCP at `.agents/mcp_config.json`, requested a fresh Gemini scope check, and pointed out that hook paths belong in adapter configuration beside the other paths. Reopened the task for source verification and configuration cleanup.
- Verified against the current official product documentation that Antigravity uses `.agents/mcp_config.json` at workspace scope and Gemini CLI uses `.gemini/settings.json` at project scope. The user was correct about both paths; implementation will make Antigravity project-local and add every native hook path to `FolderConfig` while keeping hook behavior in the concrete adapter.
- Added `HookFilePath` to `FolderConfig`; all seven constructors now declare their project-local hook target beside `MCPFilePath`, and each concrete adapter resolves that configured path inside its own sync/removal methods. Changed Antigravity MCP installation from the global Gemini path to project-local `.agents/mcp_config.json` and extended structural coverage for configured hook paths and project-scoped Antigravity MCP reconciliation.
- Focused adapter, session-hook, memory, and CLI tests pass after the path-metadata and Antigravity scope correction.
- `git diff --check` and the complete `go test ./...` suite pass after the final correction. Updated the durable adapter-ownership memory to record the verified Antigravity and Gemini project paths and the `HookFilePath` configuration boundary.
- Ran final `graphit_sync` successfully, then exercised the current source sync with a stable launcher path. Adapter synchronization completed and generated `.agents/mcp_config.json` with the Graphit MCP in the project. The optional vector phase of the source-only `go run` reported the expected missing `lancedb` build tag, while the MCP-driven full sync had already completed successfully.
- User corrected the remaining global-scope legacy: `_graphitManagedMcpKeys` represented ownership claims from multiple installed projects and is unnecessary now that every adapter writes a project-local MCP file. Reopened the task to remove that metadata while retaining safe cleanup of Graphit- and Hub-managed server names.
- Removed the public managed-key helper and project-ID claim logic. Standard JSON adapters now use the same project-local runtime manifest strategy as Codex and OpenCode; UI discovery treats every non-core server in the project's MCP file as local. Added replacement/removal, user-preservation, and manifest coverage. Focused adapter, Hub, brand, and live-search preparation tests pass.
- `git diff --check` and the complete `go test ./...` suite pass after removing the cross-project MCP claim model.
- Final `graphit_sync` completed and generated project MCP files were verified to contain the Graphit server without a `ManagedMcpKeys` field; no public `ManagedMCPKey` helper remains.
- User clarified that the project is still in development and requires no backward compatibility. Reopened the task to delete the legacy MCP metadata reader and migration test rather than retaining dead compatibility behavior.
- Deleted the legacy MCP metadata reader and its migration test. Current reconciliation now reads only the project-local runtime manifest and contains no code path for the old configuration field.
- Focused tests, `git diff --check`, and the complete `go test ./...` suite pass with legacy compatibility removed.
- User identified that OpenCode's native project schema uses `mcp`, so the adapter must not duplicate servers into `mcpServers`. Reopened the task to remove the generic copy from OpenCode sync/removal and pin the native-only shape in tests.
- OpenCode sync and removal now touch only the native `mcp` object. Tests assert that Graphit and Hub servers appear there, `mcpServers` is never emitted, and unrelated OpenCode settings plus user-owned native MCP entries are preserved.
- Focused OpenCode adapter coverage passes. The first complete-suite run reached an unrelated native `SIGSEGV` in `internal/ast` while every other package passed; rerunning that package in isolation to verify whether the failure is transient.
- The isolated `internal/ast` rerun passed, followed by a successful complete `go test ./...` run. The initial native crash was transient and unrelated to the OpenCode adapter change.
- User reported that invoking `graphit _session-hook --format first-invocation` interactively hangs with no output. Reopened the task to inspect stdin handling and guarantee prompt termination outside a native hook pipeline.
- Reproduced the hang under a PTY and confirmed `io.ReadAll` waited forever for terminal EOF. The command now detects character-device stdin: manual `first-invocation` calls use a deterministic invocation-zero payload, other formats need no input, and pipe/file inputs are still read normally.
- Repeated the exact source command under a PTY after the fix; it printed the `injectSteps` payload containing the ordered memory protocol and exited with status zero without input.
- Added command coverage for character-device/no-input execution and piped non-first invocation input. Focused tests, `git diff --check`, and the complete `go test ./...` suite pass.
