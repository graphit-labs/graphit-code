---
title: Confirm default IDE and CLI resolution
status: done
created: 2026-08-24
updated: 2026-08-24
tags: [config, ide, cli, fallback, verification]
---

# Confirm default IDE and CLI resolution

## Objective

Confirm from the implementation, its call chain, and automated tests whether the default IDE and CLI values participate in configuration fallback resolution when higher-priority values are absent. This is an evidence-only investigation: no resolver behavior will be changed unless a defect is found and a separate implementation request authorizes a fix.

## Plan & Task Breakdown

- [x] **T1 — Establish the documented contract** — Read the indexed configuration specification and identify the promised precedence for IDE and CLI resolution.
- [x] **T2 — Trace the implementation** — Query the AST for the resolver entities, their callers, and their dependencies; read only the relevant indexed source ranges.
- [x] **T3 — Verify executable evidence** — Identify and run the focused tests that exercise compiled/default IDE and CLI fallbacks.
- [x] **T4 — Record and report the conclusion** — Document the exact chains, any caveats or gaps, validation results, and persistent project knowledge.

## Implementation Details

The AST trace established three connected layers:

1. `internal/config/config.go:ResolveConfig` resolves inline → environment → project → global → compiled defaults.
2. `DefaultIDE` and `DefaultCLI` use that resolver and retain terminal fallbacks. `DefaultCLI` additionally derives a CLI from the fully resolved IDE before returning `claude`.
3. `internal/ai/ai.go:NewClientFromConfig` passes the resolved default CLI into `internal/ai/cli.go:tryFallbackCLI`, which tests the configured CLI first, the default IDE's equivalent CLI second, and generic candidates afterwards.

The configuration specification now records these effective chains, including the behavior when a configured executable is absent from `PATH` and the build-time `COMPILE_CONFIG` requirement.

## Use Cases

### UC-01: Resolve IDE and CLI when higher-priority configuration is absent
- **Actor**: Graphit command runtime.
- **Preconditions**: No applicable flag, inline value, environment value, project value, or global value is present for the setting being resolved.
- **Main Flow**:
  1. The runtime invokes the IDE or CLI resolver.
  2. The resolver walks its configured precedence chain.
  3. The resolver returns the compiled/default value, a value derived from the resolved IDE, or the terminal hard-coded fallback according to the implemented contract.
- **Alternative Flows**: A higher-priority source terminates the chain before defaults are consulted.
- **Error Scenarios**: A malformed or absent compiled default must not prevent the terminal fallback from producing a usable value.
- **Postconditions**: The selected IDE and CLI are deterministic and their source in the precedence chain is understood.
- **Affected Files**: `internal/config/config.go`, `cmd/graphit/commands/setup.go`, `internal/ai/ai.go`, and `internal/ai/cli.go` implement the confirmed behavior; no application source was modified.

## Test Cases & Acceptance Criteria

### Feature: Default IDE and CLI fallback resolution
Ref: UC-01

#### Scenario: Compiled IDE default is used after higher-priority sources are absent

```gherkin
Given no explicit or persisted IDE configuration exists
  And the binary contains a compiled IDE default
When the IDE resolver runs
Then it returns the compiled IDE default before its terminal fallback
```

#### Scenario: CLI default is used or derived after higher-priority sources are absent

```gherkin
Given no explicit or persisted CLI configuration exists
When the CLI resolver runs with a resolved IDE
Then it follows the implemented compiled-default and IDE-derived fallback order
  And returns the terminal CLI fallback only if all earlier sources are empty
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/confirm-default-ide-cli-resolution.md` | Created | Preserve the investigation, evidence, and conclusion. |
| `docs/specs/config_module.md` | Modified | Document the effective default IDE/CLI resolution and executable fallback chains. |
| `docs/tasks/backlog/add-a-direct-regression-test-for-compiled-cli-fallback-resol.md` | Created | Queue explicit compiled-CLI regression coverage without widening this read-only investigation into a source change. |

## Trade-offs & Decisions

- Treat documentation as the intended contract and source/tests as executable evidence; any disagreement will be reported rather than silently interpreted.
- Keep the task read-only with respect to application behavior because the request asks for confirmation, not a code change.
- Distinguish compiled defaults from terminal fallbacks: the Makefile injects only the free-form `COMPILE_CONFIG` string and does not define separate `DEFAULT_IDE`/`DEFAULT_CLI` variables.

## Technical Debt

- [ ] There is no dedicated assertion that sets `CompiledDefaults` to a `cli=...` value and calls `ResolveCLI`; queued as `docs/tasks/backlog/add-a-direct-regression-test-for-compiled-cli-fallback-resol.md`.

## System Knowledge

- Global `ide` and `cli` values written by setup are active runtime inputs, not prompt-only metadata.
- Effective `DefaultIDE()` order: `GRAPHIT_IDE` → global `ide` → compiled `ide` → `claude`.
- Effective `DefaultCLI()` order: `GRAPHIT_CLI` → global `cli` → compiled `cli` → mapping from `DefaultIDE()` → `claude`.
- AI executable lookup order: global `ai.cli` if present, otherwise `DefaultCLI()`; then the default IDE's mapped CLI if distinct; then the built-in candidate list. Missing executables fall through because each candidate is checked with `exec.LookPath`.
- Build-time IDE/CLI defaults exist only when explicitly included in `COMPILE_CONFIG`; the ordinary build leaves that string empty.

## Progress Log

### 2026-08-24

- Opened the investigation after the user asked whether the default IDE/CLI is actually in the fallback chain.
- Searched project memory and found no prior memory that precisely answers this question.
- Located indexed configuration documentation for follow-up verification.
- Confirmed the documented generic chain and IDE/CLI-specific chains in the configuration wiki.
- Queried the AST for resolver definitions, callers, callees, and test reachability; read the indexed implementations and focused test sources.
- The AST cannot model Makefile variable expansion, so a scoped textual fallback confirmed that `CompiledDefaults` receives `COMPILE_CONFIG` and that no separate `DEFAULT_IDE`/`DEFAULT_CLI` Make variables exist.
- Ran `go test -count=1 ./internal/config ./internal/ai`; both packages passed.
- Updated `docs/specs/config_module.md` with the effective config and executable discovery orders.
- Persisted the confirmed runtime chain as project memory `01M0TMWEW39FX3F7XDHZJJMK50`.
- Deferred the missing direct compiled-CLI regression assertion to the improvement backlog instead of modifying application tests outside the confirmation request.
- `git diff --check` passed for all documentation touched by this investigation.
- The project-wide knowledge lint still reports its existing broad baseline of 800 findings (primarily broken generated links and missing `updated` metadata); this investigation did not expand scope into repairing that global backlog.
- Post-task reflection created one project memory and one deferred backlog item; no reusable IDE artifact was warranted for this one-off resolver trace. The daemon is running, but dream status is `inactive`, so no automatic pickup is promised.
- Completed the investigation; application behavior was not changed.
