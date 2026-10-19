# Global native instructions

The Global install/sync consumer publishes the maintained native Role and five module skills, including their ten companions, through the existing Core writer. See the [Root module-skill contract](../specs/rust_root_module_skill.md).

Use an explicit destination:

~~~sh
graphit roles install --destination global --agent codex --role scribe
graphit roles sync --destination global --agent qwen --role scribe --skill-alias task=team-task
~~~

Global defaults do not infer a PROJECT from the request's working directory. Compiled defaults and captured Global raw instructions work without a PROJECT. To select a PROJECT's raw overrides or its exact installed Hub bundle, name that origin explicitly:

~~~sh
graphit roles sync --destination global --agent codex --role scribe --source-project /work/service --skill-bundle task=team/task@one
~~~

The PROJECT is independently selected from the daemon's retained versions and authorized roots. Its raw inputs can supply authored instructions; it never grants a Global PROJECT context or storage permission. Project raw overrides keep their normal priority above Global raw, a selected Hub source and compiled defaults. License, compatibility, metadata and companions use the maintained SkillDescriptor/renderers. Selected remote sources retain the current origin/PROJECT admission and existing cache proof.

Global profiles do not embed a selected PROJECT path/ID/version, CWD, StandingContext or mandatory Memory. Kiro Role hooks use the maintained neutral native invocation with installation selection and recipient Role. Runtime credentials/context are still supplied by actual run/hook owners. Knowledge Global guidance resolves DOCS_ROOT through module_skill for the active PROJECT; it does not freeze the origin PROJECT's docs_dir. Project/runtime rendering retains its actual docs_dir. Custom Global module-skill raw templates can use publication_scope=global to choose runtime guidance; a required per-PROJECT variable is not replaced with a fictional value.

roles run remains a PROJECT operation. --destination global is rejected before daemon bootstrap/transport/native launch. This is separate from the host's ability to discover a published global profile.

## Captured destinations

The bootstrap owner retains its optional OS home, startup directory and only the seven relevant directory overrides. They are captured once before serving. Requests and recovery do not rediscover HOME or reread ENV, and GlobalCache is not treated as a native config directory. Relative native overrides are resolved against that captured startup directory. Only the concrete agents/skills directories enter the publication scope; observing creation ancestors does not grant HOME access.

| Binding | Default agent directory | Default skill directory | Captured override |
| --- | --- | --- | --- |
| Codex | ~/.codex/agents | ~/.agents/skills | CODEX_HOME changes the native agent/config root; authored personal skills stay under the OS home |
| Claude | ~/.claude/agents | ~/.claude/skills | CLAUDE_CONFIG_DIR |
| Cursor | ~/.cursor/agents | ~/.cursor/skills | No undocumented skill/agent relocation is inferred from cli-config settings |
| Qwen | ~/.qwen/agents | ~/.qwen/skills | QWEN_HOME, including its documented empty/unset, relative and ~ forms |
| AGY/Antigravity CLI | ~/.gemini/config/agents | ~/.gemini/antigravity-cli/skills | No undocumented override |
| Kiro V3 | ~/.kiro/agents | ~/.kiro/skills | KIRO_HOME |
| OpenCode 2 | ~/.config/opencode/agents | ~/.config/opencode/skills | OPENCODE_CONFIG_DIR, otherwise XDG_CONFIG_HOME/opencode |
| Kimi Code 2.2 | ~/.kimi-code/agents | ~/.kimi-code/skills | KIMI_CODE_HOME |

The paths are native Path values, using the captured OS home on Windows, macOS and Linux. OpenCode's own V2 global-roots source uses XDG_CONFIG_HOME or home/.config on these systems; the publisher does not substitute AppData or Library by guess. Path cleaning uses path-clean; Qwen's documented ~ expansion is explicit, not a feature of that library. A configured empty directory is not silently redirected except where the host documents empty as unset. Additional host discovery directories are not additional Graphit destinations.

These destination contracts come from primary documentation: [Codex skills](https://learn.chatgpt.com/docs/build-skills), [Codex agents](https://learn.chatgpt.com/docs/agent-configuration/subagents), [Codex environment](https://learn.chatgpt.com/docs/config-file/environment-variables), [Claude skills](https://code.claude.com/docs/en/skills), [Claude environment](https://code.claude.com/docs/en/env-vars), [Cursor skills](https://cursor.com/docs/skills), [Cursor agents](https://cursor.com/docs/subagents), [Qwen settings](https://qwenlm.github.io/qwen-code-docs/en/users/configuration/settings/), [Qwen agents](https://qwenlm.github.io/qwen-code-docs/en/users/features/sub-agents/), [AGY CLI agents](https://antigravity.google/docs/cli/subagents), [AGY skills](https://antigravity.google/docs/skills/), [Kiro scopes](https://kiro.dev/docs/cli/chat/configuration/), [Kiro home setting](https://kiro.dev/docs/cli/reference/settings/), [OpenCode V2 skills](https://opencode.ai/v2/docs/skills/), [OpenCode V2 global roots](https://github.com/anomalyco/opencode/blob/dev/packages/util/src/global-roots.ts), [OpenCode global override](https://github.com/anomalyco/opencode/blob/dev/packages/util/src/global.ts), [Kimi 2.2 agents](https://github.com/MoonshotAI/kimi-code/blob/472192c34c1efad89a369870eff0da0b708ce9bb/docs/en/customization/agents.md), and [Kimi 2.2 skills](https://github.com/MoonshotAI/kimi-code/blob/472192c34c1efad89a369870eff0da0b708ce9bb/docs/en/customization/skills.md).

## Owned publication and control data

Native instruction control data uses the concrete Installation namespace runtime/native-instructions. The default FILE/cache writer keeps runtime/artifact-store.json. The real ModelProvisioner uses runtime/models so its models-only root set cannot collide with the cache's installation root set. All three reuse the same Core writer, Installation identity, exact scope validation, locks, payload integrity, pending recovery and known effect reporting. There is no public arbitrary namespace, new writer or old-data importer.

All native instruction bindings share the stable sorted set of captured agent/skill roots. A shared physical skill directory therefore shares ownership by its scope-root index and module, rather than adopting equal foreign bytes. Distinct directories remain distinct, and role keys preserve distinct native filenames. Alias retargeting, existing pins, recovery and GC continue through Core. Foreign files remain untouched; a failure can report already configured/published Role effects instead of pretending the entire operation was atomic.

The focused consumer uses real CLI/HTTP and writers with two retained PROJECT origins, actual local Hub publication, shared Codex/Qwen skill destination, neutral Kiro profile, aliases and foreign preservation. The namespace regression captures the real ModelProvisioner, writes small local documents through its private writer plus FILE/cache/instructions and reopens/recovers all three. It does not fabricate model weights or repeat unchanged BF16/inference qualification. No agent binary, provider account or plan is required. OS path documentation is not evidence of executing native provider processes on every platform; native command/platform qualification remains its own outcome.

## Current qualification

The existing focused executions below each passed one case, with zero failed
or ignored cases and empty stderr. Wall time includes the command boundary;
it is not a benchmark or individual SDK-operation latency.

| Scope | Run and wall time | Actual evidence |
|---|---|---|
| Core concrete namespaces and exact binding/complete receipt shape | `core-concrete-namespaces50r`, code 0, 162 ms | stdout (`../../.build/rust-records-data/knowledge-index-source/core-concrete-namespaces50r.stdout.log`; historical local artifact unavailable), result (`../../.build/rust-records-data/knowledge-index-source/core-concrete-namespaces50r.result.json`; historical local artifact unavailable) |
| Global real CLI/HTTP publication, neutral/shared destinations and foreign preservation | `global-publication50s`, code 0, 9172 ms | stdout (`../../.build/rust-records-data/knowledge-index-source/global-publication50s.stdout.log`; historical local artifact unavailable), result (`../../.build/rust-records-data/knowledge-index-source/global-publication50s.result.json`; historical local artifact unavailable) |
| Actual Models, FILE/cache and native-instructions journals coexist and reopen | `model-namespace50t`, code 0, 612 ms | stdout (`../../.build/rust-records-data/knowledge-index-source/model-namespace50t.stdout.log`; historical local artifact unavailable), result (`../../.build/rust-records-data/knowledge-index-source/model-namespace50t.result.json`; historical local artifact unavailable) |

Root now has seven distinct functional focals: the four individual PASS in
historical mixed Root50v, authority/reference/override Root50x, and the repaired
catalogue and alternate-Brand exact cases in Root51d (403 ms and 236 ms).
The 51d aggregate is code 0, 680 ms wall time, two PASS and zero failed/ignored.
The integrated Brand/fixture delta compiled in Root51c, code 0 and 232231 ms,
with empty stdout and 83578 bytes of compilation stderr. The
[Root spec](../specs/rust_root_module_skill.md) links all actual results/logs.
Root50v remains code 101, four PASS and one HTTP413 failure at 546 ms; the
later corrected exact PASS does not relabel that historical group. Affected
Global/App/Root strict qualification now passes in the explicit51z scope.

Affected strict51g failed with code 101 at 178164 ms wall time on the single
`module_skill.rs` Clippy `collapsible_if`. The coordinator applied the equivalent
let-chain only, retained the seven functional proofs without replay and
reported scoped rustfmt/check passing. The affected retry51z now passes:
code 0, null signal, 43859 ms wall time and 84824 bytes of compilation stderr
without errors. Its scope is Auth/App/Data/Core/FS libraries and the
`managed_roles` target with `-D warnings`. Formatting was not the strict PASS;
51z is the real passing execution. The Root spec above links its result and
stderr, preserving the failed51g evidence.
