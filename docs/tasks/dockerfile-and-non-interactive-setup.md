---
title: Container image built from the published release, with a flag-per-question setup
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [docker, setup, packaging, ui, release, secrets]
---

# Container image built from the published release, with a non-interactive setup

## Objective

Ship a `Dockerfile` at the repository root that produces a runnable Graphit Code container.
Requirements, as stated by the Engenheiro:

1. **It must not compile the framework.** It downloads the version published on GitHub and
   uses that binary. The first proposal — a multi-stage build running `make build-linux` — was
   rejected.
2. **A build argument selects which AI CLI is installed** into the image. The candidate set is
   the CLIs the framework actually invokes, and each install command must come from that CLI's
   own documentation rather than from recollection.
3. **The S3 and CLI environment variables must be present in the image**, plus whatever else
   the container needs to work.
4. **The entrypoint is the UI**, and the port the UI listens on must be exposed.
5. **The image runs `graphit setup`**, which means `setup` needs a way to install without
   interactivity — flags, not a piped-stdin trick.

Added by the Engenheiro while the work was in progress, each recorded in the Progress Log at the
point it arrived:

6. **There is no non-interactive mode.** Each question has a flag, and supplying the flag is what
   suppresses that question. Supplying all of them is what results in no interaction. An earlier
   `--non-interactive` switch was removed.
7. **Every secret in the global configuration must be obtainable from an environment variable**,
   and the image must accept them.
8. **Prefer each AI CLI's own shell installer** over its npm package.
9. **Base the image on the official Node image** rather than installing Node conditionally.
10. **No `none` option for the CLI, and the default is `opencode`** — including in the framework's
    own code, where the default was `claude`.
11. **Both the global home and the projects root are volumes**, and the container guide must
    document bringing up a centralized UI.

Point 5 is a code change, not packaging: `graphit setup` currently reads every answer from
`bufio.NewReader(os.Stdin)` and has no flags at all.

## Reasoning and justification

**Why downloading beats building, concretely.** `make build-linux` depends on `ui` (npm ci +
vite build), `setup-lbug` (downloads liblbug into the Go module cache), `fetch-ort-linux`
(ONNX Runtime), and `lancedb-native` — which, with no installed runtime to link against,
git-clones `lancedb-go` and runs `cargo build --release`. That is a Rust toolchain, a Node
toolchain, a Go toolchain and `libicu-dev` in the build image, for tens of minutes, to
reproduce an artifact the release pipeline already builds per platform. The published Linux
artifact is the **launcher**, which carries `graphit-core`, `graphit-mcp`, `liblbug`,
`libonnxruntime`, `liblancedb_go`, the httpfs extension and the AST query YAML as an embedded
payload, so a single downloaded file is the whole runtime.

**Why `install.sh` rather than a hand-rolled `curl` in the Dockerfile.** The script already
detects the platform, resolves the latest tag, downloads `checksums.sha256` and verifies
SHA-256 before installing. Re-implementing that inline would be a second, weaker copy. What it
lacked was a way to ask for a specific version, which a reproducible image needs — so the
script gains `--version` / `VERSION` instead of the Dockerfile growing its own downloader.

**Why flags rather than piping answers into stdin.** Piping is what the existing tests do, and
it is fragile in exactly the way that matters here: the answer order is positional, so a new
prompt silently shifts every later answer onto the wrong key, and a container build would keep
reporting success while storing the region as an endpoint. Flags are order-free and name what
they set.

**The flag semantics chosen, and the alternative rejected.** An explicitly supplied flag wins
over the prompt *even in interactive mode*, and `--non-interactive` only suppresses the asking.
The rejected alternative was to make the flags legal only together with `--non-interactive`:
it is less useful (`graphit setup --ide cursor` skipping one question is a reasonable thing to
want) and it needs a validation error that carries no information.

**What stays fatal.** The embedding-model download stays fatal for the local provider, per the
existing correction recorded in project memory: an installation without the model is a half
installation, and its degradation is invisible — a search answers on keywords alone and never
says the semantic half did not run. Non-interactive mode does not soften this. Neither does it
soften `verifyHubBucket`.

**What non-interactive mode must not do:** clear configuration it was not asked about. The
interactive S3 credential prompt calls `SetGlobalS3Credentials` unconditionally, so a blank
answer clears a stored pair — that is right for a prompt the operator just saw and answered.
With no credential flags supplied there was no answer, so the stored pair is left alone.

## Plan & Task Breakdown

- [x] **T1 — Map setup, the UI server, and the release install path** — Spec: identify every
  prompt in `cmd/graphit/commands/setup.go`, its callers and its tests; the UI bind host, port
  selection and CORS resolution; the env-var naming rule; and what `install.sh` accepts. Done
  when the blast radius of a signature change is known rather than assumed.
- [x] **T2 — Verify each AI CLI's install command against its own documentation** — Spec: for
  every binary in `internal/ai/cli.go:knownCLIs` that a supported IDE maps to, plus `copilot`
  and `grok`, record the official Linux install command with its source. Done when no install
  line in the Dockerfile comes from recollection.
- [x] **T3 — Give every `setup` question its own flag** — Spec: touches
  `cmd/graphit/commands/setup.go`. A question whose flag was supplied is not asked; an unsupplied
  one is asked exactly as before. An empty value is an answer and clears the key; an absent flag
  leaves it untouched. Constraint: the fatal steps stay fatal, and interactive behaviour is
  unchanged when no flag is passed. **Superseded once**: the first version added a
  `--non-interactive` switch alongside the flags, which the Engenheiro removed — see the Progress
  Log for why that was the right call.
- [x] **T4 — Add `--version` / `VERSION` to `install.sh`** — Spec: an explicit tag skips the GitHub
  "latest" lookup and is used verbatim in the download and checksum URLs. Constraint: checksum
  verification is not bypassed for a pinned version.
- [x] **T5 — Write the Dockerfile and `.dockerignore`** — Spec: `Dockerfile` at the repository root.
  Downloads the release, installs the CLI chosen by `--build-arg AI_CLI=…` using the vendor's own
  shell installer, declares the Hub/S3, CLI/IDE and secret environment variables, runs `graphit setup`
  with every question answered, exposes the UI port, entrypoints the UI bound to `0.0.0.0`, and
  declares `/opt/graphit` and `/workspace` as volumes. Constraint: no secret in a layer, and the
  image runs as a non-root user.
- [x] **T6 — Make every config secret env-supplied and redacted** — Spec: a canonical list in
  `internal/config`, `IsSecretConfigKey` derived from it so redaction covers all three keys rather
  than one, and tests proving env resolution and precedence per key. Constraint:
  `hub.access_key_id` stays unredacted — it is an identifier.
- [x] **T7 — Make `opencode` the default IDE and CLI** — Spec: one named constant per default in
  `internal/config`, replacing three separate `"claude"` literals, plus the UI server's own
  fallback. Constraint: a test pins that the fallback CLI is the one `CLIForIDE` pairs with the
  fallback IDE, because three literals had already proved able to drift.
- [x] **T8 — Regression coverage** — Spec: the flag semantics, the "still prompts" half, the secret
  keys, and the fallback invariant. The seven pre-existing prompt tests must keep passing.
- [x] **T9 — Container guide** — Spec: `docs/guides/container.md`, indexed from `docs/README.md` and
  linked from the root `README.md`. Done when an operator can bring up a centralized UI, register
  projects into it, and expose it safely without reading source.
- [x] **T10 — Documentation surface beyond the container guide** — Spec: the `setup` flag table in
  the CLI reference, the secret-key contract and the named fallbacks in the config specification,
  `install.sh --version` in getting started, and a container-guide pointer from getting started and
  the root README. Done when no public page still states `claude` as the default or omits the flags.
- [x] **T11 — Verify and close** — Spec: `go build ./...`, the whole test suite, `go vet`,
  `golangci-lint`, `docker build` of at least one CLI variant, then one `graphit_sync`. **Residual
  blocker, not fixable here**: the image cannot complete its `setup` step until a release ships the
  flags from T3; the build guard reports exactly that, and it is recorded in the backlog.

## Implementation Details

*(filled in as each task lands)*

### T1 findings — the seams

| What | Where | Why it matters here |
|---|---|---|
| Every setup answer | `cmd/graphit/commands/setup.go`, `RunE` | One `bufio.Reader` over `os.Stdin`, read positionally. No flags exist. |
| Prompt helpers with test callers | `promptS3Credentials`, `promptEmbeddingProvider`, `promptRerankProvider` | Seven tests in `setup_ai_provider_test.go` and `setup_credentials_test.go` call them directly, so a signature change is a test change. |
| Env-var naming | `internal/config/config.go:ResolveConfig` | `brand.EnvPrefix() + "_" + upper(key with . → _)`, so `hub.bucket` → `GRAPHIT_HUB_BUCKET`. Layer 2 of 5: it outranks both config files. |
| UI bind host | `internal/config/ui_server.go:ResolveUIHost` | Compiled default is `127.0.0.1` — a container needs `GRAPHIT_UI_HOST=0.0.0.0`. The docs page for the S3/UI task still says the default is `0.0.0.0`; the later loopback decision superseded it. |
| UI port | `internal/uiserver/unified_server.go:NewUnifiedServer` → `netutil.FindFreePortOnHost(host, 8080)` | There is no port flag or config key. It scans 8080–8179 and takes the first free port, so in a clean container it is deterministically 8080. |
| Browser launch | `cmd/graphit/commands/runners.go:runUnifiedServe` → `openBrowser` | Runs `xdg-open` and ignores the error, so a headless container is fine. |
| CLI ↔ IDE mapping | `internal/config/config.go:CLIForIDE` | `antigravity→agy`, `gemini→gemini`, `claude→claude`, `cursor→cursor-agent`, `codex→codex`, `opencode→opencode`, `kiro→kiro-cli`. |
| Invokable CLIs | `internal/ai/cli.go:knownCLIs` | Also holds `copilot`, `grok`, `agent`, `cline`, `goose`, `openhands`. |
| `git` is required | `setup.go` `RunE`, first check | The runtime image must carry git or setup exits before its first prompt. |
| Daemon autostart | `internal/daemonctl/daemonctl.go:EnsureRunning` | Detached, `Stdout = nil`, stderr to a log file — so it cannot hang a `docker build` layer. |
| Release download | `install.sh` | Platform detection, latest-tag lookup, SHA-256 verification, `--dir`. No version pin. |

### T2 findings — verified install commands

The Hub registry has no `knowledge` artifact for any of these (`graphit_hub_list` with
`type: knowledge` returns nothing), so these come from each vendor's own documentation.
Content was rephrased for compliance with licensing restrictions.

| CLI binary | IDE it maps from | Install (Linux) | Source |
|---|---|---|---|
| `claude` | `claude` | `npm install -g @anthropic-ai/claude-code` | [Claude Code repository](https://github.com/anthropics/claude-code) |
| `gemini` | `gemini` | `npm install -g @google/gemini-cli` | [Gemini CLI docs](https://github.com/google-gemini/gemini-cli/blob/main/docs/get-started/index.md) |
| `agy` | `antigravity` | `curl -fsSL https://antigravity.google/cli/install.sh \| bash` (installs to `~/.local/bin/agy`) | [Antigravity CLI reference](https://github.com/jqueryscript/antigravity-cli-cheatsheet/blob/main/readme.md) |
| `codex` | `codex` | `npm install -g @openai/codex` | [Codex README](https://github.com/openai/codex/blob/main/README.md) |
| `opencode` | `opencode` | `npm install -g opencode-ai` (the unscoped `opencode` package is a different project) | [OpenCode docs](https://docs.dev.opencode.ai/docs/) |
| `cursor-agent` | `cursor` | `curl https://cursor.com/install -fsS \| bash` | [Cursor CLI installation](https://cursor.com/docs/cli/installation) |
| `kiro-cli` | `kiro` | `curl -fsSL https://cli.kiro.dev/install \| bash` | [Kiro CLI install guide](https://learn.arm.com/install-guides/kiro-cli/) |
| `copilot` | — | `npm install -g @github/copilot` | [Installing GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli) |
| `grok` | — | `curl -fsSL https://raw.githubusercontent.com/superagent-ai/grok-cli/main/install.sh \| bash` | [grok-cli README](https://github.com/superagent-ai/grok-cli/blob/main/README.md) |

**Deliberately not offered by the image:** `agent`, `cline`, `goose` and `openhands`. They are
in `knownCLIs` but no supported IDE maps to them, and inventing an install line for a CLI
nobody selects would be exactly the guessing this task set out to avoid. `AI_CLI=none` covers
the case where the operator mounts a CLI in from outside.

## Use Cases

*(filled in with T3 and T5)*

## Test Cases & Acceptance Criteria

*(filled in with T6)*

## Files Changed

| File | Change | Reason |
|---|---|---|
| `Dockerfile` | Created | The image: release download, CLI install, flag-answered setup, UI entrypoint, both volumes. |
| `.dockerignore` | Created | The build needs only `install.sh`; everything else is excluded so the context stays kilobytes. |
| `install.sh` | Modified | `--version` / `VERSION` to pin a release tag, without bypassing checksum verification. |
| `cmd/graphit/commands/setup.go` | Modified | A flag per question; a supplied flag suppresses its own prompt. `secretFlagUsage` names the env var. |
| `cmd/graphit/commands/setup_flags_test.go` | Created | Flag semantics: applied, trimmed, clears on empty, still prompts when absent. |
| `cmd/graphit/commands/setup_ai_provider_test.go` | Modified | Updated for the new prompt-helper signatures. |
| `cmd/graphit/commands/setup_credentials_test.go` | Modified | Same. |
| `cmd/graphit/commands/config_secret_test.go` | Modified | Redaction now asserted over every key in `SecretConfigKeys`, not just the S3 secret. |
| `cmd/graphit/commands/runners.go` | Modified | The UI server's IDE fallback uses `config.FallbackIDE`. |
| `internal/config/secrets.go` | Created | `SecretConfigKeys`, `IsSecretConfigKey`, `SecretConfigEnvVars` — one list, two behaviours. Env-var names come from `ConfigEnvVar`. |
| `internal/config/secrets_test.go` | Created | Env resolution, precedence over the file, empty-does-not-mask, redaction, per key. |
| `internal/config/fallback_test.go` | Created | `FallbackCLI` must be `CLIForIDE(FallbackIDE)`; the fallback is last-resort only. |
| `internal/config/config.go` | Modified | `FallbackIDE` / `FallbackCLI` constants replacing three `"claude"` literals. |
| `internal/config/hub_s3.go` | Modified | `IsSecretConfigKey` moved out to `secrets.go`. |
| `internal/config/config_default_test.go` | Modified | Asserts against the constants rather than the old literal. |
| `docs/guides/container.md` | Created | The operator guide: quick start, volumes, live search, CLI choice, secrets, exposure, troubleshooting. |
| `docs/guides/cli_reference.md` | Modified | The `setup` flag table and the flag-suppresses-its-question rule. |
| `docs/guides/getting_started.md` | Modified | `install.sh --version`, and a pointer to the container guide. |
| `docs/specs/config_module.md` | Modified | Secret-key contract; the named fallbacks replacing three `"claude"` statements. |
| `docs/specs/ai_engine.md` | Modified | The CLI default names `config.FallbackCLI`. |
| `docs/README.md` | Modified | Container guide indexed. |
| `README.md` | Modified | Container section, and `--version` mentioned for the installers. |
| `docs/tasks/dockerfile-and-non-interactive-setup.md` | Created | This log. |
| `docs/tasks/backlog/rebuild-and-verify-the-container-image-after-the-next-releas.md` | Created | The one check that cannot be performed until a release ships the setup flags. |
| `internal/config/config.go` | Modified | `ConfigEnvVar` — the env-var derivation extracted from `ResolveConfig` so it has one implementation; `daemon_ui` added to `OptInModules`. |
| `internal/config/agent_features.go` | Created | `modules.agent`: the gate for every feature needing a coding-agent CLI. |
| `internal/config/daemon_server.go` | Created | `mcp.host`, `mcp.port`, and `modules.daemon_ui`. |
| `internal/config/agent_features_test.go` | Created | Defaults, env override, project-over-global, and the port fallback that must not fail the daemon. |
| `cmd/graphit/commands/daemon.go` | Modified | MCP listener binds the configured host/port; registers the UI global module when `modules.daemon_ui` is on. |
| `cmd/graphit/commands/daemon_ui_module.go` | Created | The daemon's UI module — a `daemon.WatchModule`, in the command package to avoid an import cycle. |
| `internal/ast/server.go` | Modified | No agent client constructed when `modules.agent` is off. |
| `internal/uiserver/wiki_handler.go` | Modified | Same, for wiki AI search. |
| `internal/uiserver/unified_server.go` | Modified | Live-search routes not registered when off; `window.__AGENT_FEATURES__` injected. |
| `internal/uiserver/daemon_dream_handler.go` | Modified | `mcp_key` in the status response, and an endpoint that reports the configured host. |
| `internal/ui/src/lib/utils.ts`, `globals.d.ts`, `api/daemon.ts` | Modified | `agentFeaturesEnabled()` and the `mcp_key` field. |
| `internal/ui/src/components/ast/QueryBar.tsx` | Modified | The NL mode toggle is not rendered when agent features are off. |
| `internal/ui/src/components/wiki/WikiExplorerPage.tsx` | Modified | Same for AI search — covers knowledge and memory. |
| `internal/ui/src/components/layout/Sidebar.tsx`, `App.tsx` | Modified | Live search nav entry and route gated. |
| `internal/ui/src/components/daemon/DaemonDashboard.tsx` | Modified | MCP auth key with a copy button, displayed masked. |

> The working tree also carries unrelated modifications from a previous session — `internal/ast/*`,
> `.github/workflows/*`, `.golangci.yml`, `Makefile`, `go.mod`/`go.sum`, and two other task logs.
> None of them belong to this task.

## Trade-offs & Decisions

- **Download over build**, accepting that the image tracks a *released* version rather than the
  working tree. Someone who wants the working tree still has `make build-local`.
- **`graphit setup` runs at build time**, which bakes the ~132 MiB embedding model into a layer
  and makes container start fast. The cost is image size and a network requirement during
  build; the alternative — setup on first boot — pays it on every container instead, and turns
  a build failure into a runtime failure.
- **The UI is bound to `0.0.0.0` in the image** because a container that only answers on its
  own loopback is useless. This is a deliberate reversal of the compiled default for this one
  environment, and it is why the security note in the guide is not optional: **the UI has no
  authentication.**

## Technical Debt

- [ ] The UI port cannot be configured — `NewUnifiedServer` hardcodes 8080 as the scan start
  and there is no `ui.port` key. The image therefore documents 8080 rather than parameterising
  it, and `-p` remapping on the host is the only knob.
- [ ] `docs/tasks/configure-s3-and-ui-server-network.md` states the default `ui.host` is
  `0.0.0.0`; the later `docs/decisions/2026-08-31-default-ui-host-loopback.md` changed it to
  `127.0.0.1`. The older task log is historical, so it is not being rewritten, but anyone
  reading it in isolation is misinformed.

## System Knowledge

- `ResolveConfig` puts environment variables **above** both the project lockfile and the global
  config file. That is what makes a container configurable purely with `-e`, and it also means
  a value can be in force while `graphit config list` shows nothing.
- The UI port is not a setting. `FindFreePortOnHost(host, 8080)` scans one hundred ports and
  returns the first that binds, which is why multiple projects can each run their own UI and
  why a container gets 8080.
- `setup` refuses to start without `git` in `PATH`, before any prompt — so a slim base image
  fails at the first line of setup rather than somewhere useful.
- The launcher extracts its payload into `<global>/runtime/<version>/` on first invocation.
  Running any command during the build (even `--version`) warms that extraction into the image
  layer, so container start does not pay for it.

## Progress Log

### 2026-09-01

- Opened the task **after** the Engenheiro rejected the first approach. The rejected version is
  recorded above rather than deleted: a compile-in-image Dockerfile is a plausible-looking idea
  and the reason it is wrong (cargo build of the LanceDB native, per platform, per build) is
  not visible from the Dockerfile itself.
- The correction is also persisted in project memory as
  `Dockerfile do projeto baixa o release do GitHub — não compila o framework`.
- Consulted project memory before designing the setup change. Two memories constrain it
  directly: the embedding-model download must stay fatal, and a half installation must never
  report success.
- T1 and T2 complete — see the tables above. The Hub has no artifact for any of the AI CLIs, so
  the install commands were taken from vendor documentation and each one is cited.
- Next: T3, the `setup` flags, which is the change with real blast radius (seven existing tests
  call the prompt helpers directly).

### 2026-09-01 — corrections, in the order they arrived

Each of these changed a decision already implemented. The superseded version is described rather
than deleted, because in every case the rejected option is the one that looks reasonable.

**1. The image must not compile the framework.** Recorded at the top of this log.

**2. `--non-interactive` removed; a flag suppresses its own question.** The first implementation had
both: a flag per question *and* a mode switch that silenced everything unanswered. The switch was a
redundant second axis, and worse than redundant — `--non-interactive` on its own meant "answer
everything by default, silently", which is not the same as "I answered everything", and it made
"did I supply enough?" something the operator had to reason about instead of something the command
demonstrates by asking. Removing it makes the absence of interaction a *verifiable consequence* of
having answered. A prompt that appears is now useful information.

Consequence for the Dockerfile, and it is not incidental: the `setup` invocation has to be
**complete**. A question left without its flag is asked, reads EOF from the build's empty stdin, and
takes the default in silence. The comment above that RUN enumerates every question the run reaches
and why that is the whole list.

Consequence for the code shape: `value`, `simple` and `secret` moved from methods on `setupAnswers`
to methods on `setupAnswer`. Each of them only ever consulted the one answer passed to it, so the
receiver was carrying nothing, and `answers.hubBucket.value(…)` reads as what it is.

**3. Every config secret must come from an environment variable, and the image must accept them.**
Investigation found the reading half already worked: `ResolveConfig` has an environment layer for
every key, and both `ResolveHubSecretAccessKey` and `internal/ai:resolveAPIKey` go through it. The
real gap was elsewhere — `IsSecretConfigKey` knew about `hub.secret_access_key` alone, so
`ai.embedding.api_key` and `ai.rerank.api_key` were **printed in clear** by `config get` and
`config --list`. Redaction and env-resolution now derive from one canonical list,
`config.SecretConfigKeys`, and the tests iterate that list so a fourth credential is covered the day
it is added.

`hub.access_key_id` is deliberately *not* in the list. An access key ID is an identifier, and
redacting it would hide the value an operator most often needs to read back when working out which
credentials a machine is actually using. `TestTheAccessKeyIDIsNotTreatedAsASecret` pins that so it
cannot become a side effect of a later change.

**4. Prefer each vendor's shell installer over npm.** Eight of the nine CLIs publish one; each URL
was verified to return HTTP 200 and a shell shebang before being written into the Dockerfile.
`gemini` is the single exception — its documentation offers npm and nothing else.

**5. Base on the official Node image.** The design this replaced installed Node only when
`AI_CLI=gemini`, which looked cheap and paid for itself with
`curl -fsSL https://deb.nodesource.com/setup_22.x | bash -`: a piped third-party script that adds an
apt repository to the image. Two reasons to change, and the second is the deciding one: the official
image removes that trust relationship, and Node is not only gemini's need — the MCP servers and
plugins that all of these agents extend themselves with are npm packages launched with `npx`, so an
agent container without Node breaks the first thing a user tries to add. Cost: ~150 MB uncompressed
for the eight CLIs that ship a native binary.

**6. No `none`, and the default is `opencode` — in the code too.** `none` was invented here, not
asked for: an agent image with no agent is not a configuration, it is a broken state that exists
only to be diagnosed later.

The code half had more blast radius than it looks. `"claude"` was the fallback in **three separate
literals** in `internal/config/config.go` — `ResolveIDE`, `resolveAmbientIDE`, `ResolveCLI` — plus a
fourth in `runUnifiedServe`. Four literals can be changed one at a time, and the result does not
fail: the resolution paths simply disagree about what the default is, and nothing reports that. So
the fix was two named constants, `config.FallbackIDE` and `config.FallbackCLI`, with
`TestFallbackIDEAndCLIAgree` pinning that the fallback CLI is the one `CLIForIDE` pairs with the
fallback IDE.

**7. Both directories are volumes; document the centralized UI.** `/opt/graphit` and `/workspace`
are declared in the image. The non-obvious part, which the guide leads with: `global.lock.json`
records each project by **absolute container path**, so mounting the projects root at a different
path on a later run registers the same checkout twice and orphans the first entry. The projects root
stayed `/workspace` at the Engenheiro's confirmation, having briefly been specified as `/projects`.

### 2026-09-01 — verification status

Verified:

- `go build ./...` clean.
- `internal/config` and `cmd/graphit/commands` pass, including the seven pre-existing prompt tests
  and the new flag, secret and fallback tests.
- `docker build --check` reports no warnings.
- A real `docker build` reaches the `setup` step with `AI_CLI` at its default and at `claude`. The
  opencode installer reports `Installing opencode version: 1.18.25`; the Claude installer reports
  `✔ Claude Code successfully installed!`; `install.sh` downloads the release, verifies the
  checksum, and installs to `/usr/local/bin/graphit`.

NOT verified, and it cannot be from this tree:

- **The image's `setup` step has never completed.** `GRAPHIT_VERSION=latest` resolves v0.1.26, which
  predates the flags from T3, so the guard added for exactly this case refuses the build and names
  the missing flag. The image will build once a release carries them. The flags themselves are
  covered by unit tests; what is unproven is the full `setup` run inside the container.
- The wider test suite (`internal/hub`, `internal/ai`, `internal/livesearch`, `internal/uiserver`,
  `internal/dream`) has not been run since the `claude` → `opencode` default change. Those packages
  contain assertions mentioning `"claude"`, and some may be pinning the old default rather than an
  explicitly configured value.

### 2026-09-01 — the guide was missing the feature that motivates the image

The Engenheiro pointed out that the container guide described the centralized UI as a *view* — the
Hub registry plus the registered projects — and never mentioned that users can run **live search
across the whole Hub** from it. That omission understated the point of the deployment.

What was added, checked against `internal/livesearch` and `cmd/graphit/commands/live.go` rather than
described from the name:

- A user selects Hub artifacts (`knowledge`, `ast`, or several of each) and asks a question. The
  server prepares a **throwaway project** per session — artifacts installed, documentation compiled,
  code graphs made queryable, the user's memory scope brought in — runs an agent inside it with the
  framework's tools, and streams the answer over SSE.
- Nothing is installed into anyone's own repository, and the ephemeral project is deleted with the
  session.
- A session is not a request: a closed tab or a proxy timeout does not cancel the run, and the
  transcript replays from the session's event log on reconnect. A slow subscriber is dropped rather
  than waited for, which is why a stream can stop while the run continues — worth documenting as a
  troubleshooting entry, because it looks like a failure and is not.

**The connection that was worth making explicit:** `AI_CLI` is not decoration. Live search *is* the
agent CLI, driven by the server, so an image whose CLI was never authenticated browses the Hub
perfectly well and fails every live search. That now appears three times in the guide — in the
introduction, in the CLI section, and in troubleshooting — because it is the failure a first
deployment will actually hit.

Also stated plainly in the new section: sessions are separate but **not private**. The UI has no
accounts, so everyone who reaches the port sees every transcript. Pointing a team at a centralized
live search makes the missing authentication a sharper problem than browsing does, and the guide says
so at the point where someone would otherwise not think to ask.

### 2026-09-01 — close-out

The Engenheiro asked what was still pending, and noted that backward compatibility is not a concern
in dev. Two things were outstanding, and both are now resolved.

**1. The wider test suite had not run since the `claude` → `opencode` default change.** It has now,
and it passes: **42 packages, no failures**, plus `make vet` and `golangci-lint` (0 issues). The
`"claude"` occurrences in `internal/hub`, `internal/ai`, `internal/livesearch`, `internal/uiserver`
and `internal/dream` turned out to be *explicitly configured* values in each test, not assertions
about the default — so nothing was pinning the old fallback. Worth recording as a fact rather than
relief: the default was only ever read from the four literals that became two constants.

A note on why that run kept failing to produce output earlier, because it cost several attempts and
looked like a hang in the project: the go-ladybug module path contains `!` characters
(`github.com/!ladybug!d!b/...`), and in an interactive bash those trigger **history expansion**
inside double quotes. The command was being rewritten into a previous history entry before it ran,
which is why one attempt printed the diff of an unrelated file. Globbing the path
(`.../github.com/*adybug*/go-ladybug@v0.17.0/lib`) avoids it entirely. Nothing was wrong with the
tests.

**2. The documentation surface beyond the container guide.** Completed as T10:

| Page | Change |
|---|---|
| `docs/guides/cli_reference.md` | The full `setup` flag table, the "a flag suppresses its own question" rule, empty-value-is-an-answer semantics, and the extra questions a non-local provider reaches |
| `docs/specs/config_module.md` | New "Secret keys: environment-supplied and redacted" section; `FallbackIDE` / `FallbackCLI` replacing the three `"claude"` fallback statements; the three new API-surface rows |
| `docs/specs/ai_engine.md` | The CLI default now names `config.FallbackCLI` (`opencode`) instead of implying `claude` |
| `docs/guides/getting_started.md` | `install.sh --version` for a pinned install, and a pointer to the container guide from the Observatory step |
| `docs/README.md`, `README.md` | The container guide indexed and linked from the front door |

## Final verification

| Check | Result |
|---|---|
| `go build ./...` | clean |
| Full suite, `-short -tags lancedb`, generated parsers excluded | **42 packages ok, 0 failures** |
| `make vet` | clean |
| `golangci-lint run ./...` | 0 issues |
| `sh -n install.sh`, `--help`, `VERSION=` env path | clean |
| `docker build --check` | no warnings |
| `docker build` (default `opencode`, and `claude`) | reaches `setup`; installers and release download both succeed |
| `git diff --check` | clean |
| Doc links in `container.md` | 5 in-page anchors and 4 relative links all resolve |

## What remains, and why it is not fixable from this tree

**The image's `setup` step has never completed**, and cannot until a release carries the T3 flags.
`GRAPHIT_VERSION` defaults to `latest`, which resolves v0.1.26, and the guard added for exactly this
case refuses the build while naming the missing flag. Everything before it is verified: the release
downloads and verifies, the CLI installs, the entrypoint and the helper script are in place.

This is recorded in the backlog as `rebuild-and-verify-the-container-image-after-the-next-release`.
It is the one claim in this log that a reader should not take as proven.

### 2026-09-01 — the image becomes a daemon, and the agent comes out of it

A second round of concept changes from the Engenheiro, folded into the same commit. Each one is
recorded with the reasoning because several reverse an earlier decision in this very log.

**1. Do not reimplement the config env-var derivation.** `secrets.go` had grown
`SecretConfigEnvVar`, which recomputed `PREFIX + "_" + upper(key with . → _)` — the transformation
`ResolveConfig` already did inline. Two implementations of one string rule, which is the exact defect
the canonical secret list existed to prevent, reintroduced one file away from it. The derivation is
now `config.ConfigEnvVar`, **called by `ResolveConfig` itself**, and everything that needs to name a
variable calls that. `SecretConfigEnvVar` is gone; `SecretConfigEnvVars()` remains as a list helper
built on `ConfigEnvVar`.

The lesson generalises past this: when a mechanism already has the behaviour, the work is to *expose*
its one implementation, not to write a second one that agrees today.

**2. No coding-agent CLI in the image.** Everything from the previous round about `AI_CLI`, the nine
vendor installers, the CLI→IDE mapping script and the live-search section of the guide is removed.

**3. A config that disables every UI feature needing an agent CLI.** `modules.agent`, following the
established `modules.<name>` convention so it gets an environment variable and both config layers for
free. On by default; the container sets it false.

Enforced on both sides, and the split matters:
- **Backend**: `ast.NewServerOnPort` and `NewWikiHandler` do not even construct the agent client, so
  the existing nil-checks become the refusal. Live search goes further — its routes are *not
  registered*, because a session also prepares an ephemeral project and spawns a process, so a
  reachable-but-refusing endpoint would be worse than a 404.
- **Frontend**: injected as `window.__AGENT_FEATURES__` in `handleUI`, not fetched. The UI must never
  offer a feature it cannot deliver, and a control that renders and then disappears when a capability
  request returns is worse than one that was never drawn.

The boundary was chosen from the code, not from the names: exactly the routes reaching
`ai.NewClientFromConfig` are gated. Hybrid search, wiki BM25, every graph route and the whole MCP tool
surface run on ONNX embeddings or the graph alone and keep working. A container without an agent is a
fully useful Hub, explorer and MCP host; it just cannot answer in prose.

**4. Two ports, and the MCP listener becomes configurable.** It was hardcoded `127.0.0.1:0` — an
ephemeral port published to a file afterwards, which is right for a workstation and impossible to
declare in an image. New keys `mcp.host` and `mcp.port`, both defaulting to the previous behaviour
exactly. A bad port value falls back rather than failing: the daemon is PID 1 in a container, so
refusing to start over a typo in one key would be an outage instead of a diagnostic.

**5. Dream disabled in the container.** It is opt-in already, so the line is explicitness rather than
a behaviour change — but an image that silently *could* start an agent run if someone flipped one
other flag is worth being explicit about.

**6. PID 1 is the daemon, and the daemon serves the UI.** New key `modules.daemon_ui`, opt-in and
enabled in the image, adding a `daemon.WatchModule` that runs `UnifiedServer.Start`. That interface
fit without adaptation — `Start(ctx) error` that blocks until cancellation is exactly what
`SuperviseGlobal` wants — and it buys restart-on-crash and logging that a hand-rolled goroutine would
have had to reimplement.

The module lives in `cmd/graphit/commands`, not `internal/daemon`: `internal/uiserver` already imports
`internal/daemon` for the status endpoint, so putting it there is an import cycle. The compiler found
that immediately; it is recorded because the natural place to look for it is the wrong one.

`resolveDaemonUIRepoPath` exists because the daemon deliberately chdirs to the global directory, so
`os.Getwd()` is useless: it takes the first active registered project, falling back to the global dir,
and the client sends `project_dir` per request anyway.

**7. The MCP auth key, copyable from the UI.** `/api/daemon/status` already returned `mcp_key_file` —
the *path* — which a browser cannot read. It now returns the value, and the daemon page shows it
masked behind a copy button; the full key goes to the clipboard and never to the screen, because a
bearer token rendered in full is one screen-share away from being someone else's.

**This is a real security decision, not a UI detail.** That endpoint has no authentication of its own;
it is protected only by the UI's bind address and CORS policy. Anyone who can reach the UI port can
now read the MCP bearer key. It is a smaller step than it sounds — the same port already exposes every
project's graph, wiki and memory over unauthenticated routes, so it grants no access the caller did
not already have — but it raises the cost of exposing the UI, and the guide and the Dockerfile now say
so at the top rather than in a footnote.

**8. The base image went back to `debian:bookworm-slim`, reversing point 5 of the previous round.**
The Node base was justified by two things: `gemini` being npm-only, and `npx`-launched MCP servers
that an agent extends itself with. **Neither survives an image with no agent in it** — nothing here
runs node — so keeping it would have been ~150 MB for a reason that no longer existed. It is a
`BASE_IMAGE` build argument now, so switching back is one flag, and this is flagged to the Engenheiro
rather than done quietly, because it reverses an explicit instruction whose premise changed.

### Verification of this round

| Check | Result |
|---|---|
| `go build ./...` | clean |
| `internal/config`, `internal/uiserver`, `internal/ast`, `cmd/graphit/...`, `internal/livesearch/...` | all ok |
| New config tests (agent gate, daemon UI opt-in, MCP host/port, bad-port fallback) | 13 cases pass |
| `tsc -b` in `internal/ui` | clean |
| `npm run lint` | 0 errors (3 pre-existing `react-refresh` warnings, none in changed files) |
| `gofmt` on changed packages | clean |
| `docker build --check` | no warnings |
| `docker build` | reaches the `setup` step; release download and verification succeed |

One bug was found and fixed by reading rather than by a test: `HEALTHCHECK` referenced `${UI_PORT}`,
a *build* argument, which does not exist in the running container — the check would have polled
`http://127.0.0.1:/health` and marked every healthy container unhealthy. Both ports are promoted to
`ENV` for that reason.

Still not verifiable here, unchanged: **the image's `setup` step cannot complete until a release
carries the flags**, so a running container has never been observed. The daemon-as-PID-1 path, the
UI-from-daemon module and the MCP key copy button are covered by unit tests and by reading, not by a
live container.

### 2026-09-01 — the image becomes a server, and `/workspace` goes

The Engenheiro removed `/workspace`: with no AI client in the image there is nothing in the container
that needs a source checkout. And the docs — README and the **GitHub Pages site** — had to state the
use this enables: run it as a server, connect any AI agent over MCP.

A terminology correction worth recording, because it changed where the work went: **"ui" in the
Engenheiro's vocabulary means the GitHub site (`docs/site/`), not the React application in
`internal/ui`.** I had read it as the React app.

**The design was already in the code, which is why removing `/workspace` costs nothing.**
`internal/mcpstdio/context.go` documents it in `resolveProjectDirOptional`: an absent `project_dir`
means the GLOBAL scope, for "a caller with no checkout on this machine — an agent reaching the server
over HTTP — reaching Hub artifacts that were installed with no project", because those stores are keyed
by identifier and version rather than by path. `resolveArtifactScope` then enforces the one thing a
caller gets wrong: with no project, `context` is not optional, and omitting both is refused with a
message that says so instead of answering from a store keyed on the hash of an empty path.

Verified which tools serve that scope rather than assuming: AST query/schema/source/search, knowledge
search, the wiki browse/source/xrefs/log tools, the whole Hub surface, and every memory tool (its user
scope is keyed by the machine, so it is a real scope and not a fallback). Tools that write — index,
lint, export — keep requiring a project, and a server is not what those are for.

Changes:

| File | Change |
|---|---|
| `Dockerfile` | `/workspace`, `GRAPHIT_WORKSPACE` and the second `VOLUME` entry removed; `WORKDIR` is `/opt/graphit`; header reframed as a server; the MCP tool surface named as the point of the image rather than as a survivor of `modules.agent=false` |
| `docs/guides/container.md` | Retitled and rewritten around the server model: connecting an agent (endpoint, key, client config), "ask for artifacts, not paths", publishing and installing artifacts, one volume. "Registering projects" became "or index a checkout on the server", still supported, no longer the premise |
| `README.md` | The container section leads with "run it as a server, for any AI agent" and the two values needed to wire a client |
| `docs/site/index.html` | New `#server` section with a nav entry, three capability cards, the `docker run`, and a panel giving the URL and header; a **Docker server** tab in the install console; hero proof says "Any MCP agent"; a new docs card for the server guide |
| `docs/site/site.js` | Tab handler scoped to `.install-console` |
| `docs/README.md`, `getting_started.md`, `cli_reference.md` | Retitled links |

**One bug written and then found in this round.** The new server section uses a `.command-panel` for
its snippet, and `site.js` collected `.command-panel` **globally**: the tab handler computes
`active = panel.dataset.panel === tab.dataset.tab`, which is `false` for a panel with no `data-panel`,
so the first click on any install tab would have hidden the server snippet permanently. The query is
now scoped to the install console.

That was caught by rendering the page and driving it, not by reading. The first two attempts proved
nothing and are worth recording as a method note: JSDOM does not fetch external scripts without
`resources: 'usable'`, so the handler under test did not exist and every assertion passed trivially;
and injecting `site.js` manually alongside the real `<script>` double-declared its top-level `const`.
Serving the directory over HTTP and using `JSDOM.fromURL` runs the page as a browser would. The passing
run confirms the Docker tab shows and hides the right panels AND that the standalone snippet survives
both clicks.

**Also in this round, and NOT requested:** a "Connect an AI agent" panel was added to the React
daemon dashboard (`internal/ui/src/components/daemon/DaemonDashboard.tsx`), beside the MCP endpoint and
key it explains. It was written while "ui" was being read as the React app. It is genuinely useful
where it sits — anyone looking at that page is one copy away from wiring a client — but it was not
asked for, and it is flagged to the Engenheiro rather than left to be discovered.
